import hashlib
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import restart_personal
import service_personal


class JobIdleObserverTests(unittest.TestCase):
    def database(self, home, workspace):
        key = hashlib.sha256(str(workspace.resolve()).encode()).hexdigest()
        path = home / 'Library/Application Support/coding-tools-mcp/harness/personal-runtime' / key / 'runtime.sqlite3'
        path.parent.mkdir(parents=True)
        connection = sqlite3.connect(path)
        connection.execute('CREATE TABLE jobs(id TEXT, task_id TEXT, state TEXT)')
        connection.commit()
        return path, connection

    def test_persistent_wal_observer_sees_each_new_commit_without_a_pinned_read_transaction(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            workspace = home / 'workspace'
            workspace.mkdir()
            database, writer = self.database(home, workspace)
            try:
                writer.execute('PRAGMA journal_mode=WAL').fetchone()
                writer.execute("INSERT INTO jobs VALUES('initial', 'other', 'exited')")
                writer.commit()
                with service_personal.JobIdleObserver(home, workspace) as observer:
                    self.assertEqual([], observer.foreign_jobs('mine'))
                    for index, state in enumerate(('queued', 'running', 'unknown')):
                        writer.execute('DELETE FROM jobs')
                        writer.execute('INSERT INTO jobs VALUES(?,?,?)', (str(index), 'other', state))
                        writer.commit()
                        self.assertEqual([str(index)], observer.foreign_jobs('mine'))
                        self.assertFalse(observer.connection.in_transaction)
                    with self.assertRaises(sqlite3.OperationalError):
                        observer.connection.execute("DELETE FROM jobs")
                self.assertIsNone(observer.connection)
                self.assertIsNone(observer.descriptor)
                self.assertEqual([('2', 'other', 'unknown')], writer.execute('SELECT * FROM jobs').fetchall())
            finally:
                writer.close()

    def test_one_connection_covers_final_guard_and_unknown_commit_prevents_restart(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            workspace = home / 'workspace'
            workspace.mkdir()
            _, writer = self.database(home, workspace)
            writer.execute('PRAGMA journal_mode=WAL').fetchone()
            writer.execute("INSERT INTO jobs VALUES('initial', 'other', 'exited')")
            writer.commit()
            restarts, sleeps = [], []
            original_connect = sqlite3.connect

            def new_work_after_second_sample(_):
                sleeps.append(True)
                if len(sleeps) == 2:
                    writer.execute('INSERT INTO jobs VALUES(?,?,?)', ('late-unknown', 'mine', 'unknown'))
                    writer.commit()

            try:
                with patch.object(service_personal.sqlite3, 'connect', wraps=original_connect) as connects:
                    with service_personal.JobIdleObserver(home, workspace) as observer:
                        with patch.object(restart_personal.time, 'sleep', side_effect=new_work_after_second_sample):
                            with self.assertRaises(ValueError):
                                restart_personal.guarded_restart(42, lambda: 42,
                                    lambda: not observer.foreign_jobs('mine'), lambda: restarts.append(True))
                    self.assertEqual(1, connects.call_count)
                self.assertEqual([], restarts)
            finally:
                writer.close()

    def test_foreign_unknown_jobs_are_blocking_and_own_task_compatibility_is_preserved(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            workspace = home / 'workspace'
            workspace.mkdir()
            database, writer = self.database(home, workspace)
            try:
                writer.executemany('INSERT INTO jobs VALUES(?,?,?)', [
                    ('own', 'mine', 'running'), ('own-unknown', 'mine', 'unknown'), ('foreign', 'other', 'unknown'),
                    ('unbound', None, 'unknown'), ('done', 'other', 'exited')])
                writer.commit()
            finally:
                writer.close()
            before = database.read_bytes()
            files_before = sorted(database.parent.iterdir())
            self.assertEqual(['foreign', 'own-unknown', 'unbound'], service_personal.foreign_jobs(home, workspace, 'mine'))
            self.assertEqual(before, database.read_bytes())
            self.assertEqual(files_before, sorted(database.parent.iterdir()))

    def test_replaced_or_missing_database_never_reads_the_old_inode_as_idle(self):
        for replace in (True, False):
            with self.subTest(replace=replace), tempfile.TemporaryDirectory() as folder:
                home = Path(folder)
                workspace = home / 'workspace'
                workspace.mkdir()
                database, writer = self.database(home, workspace)
                writer.close()
                with service_personal.JobIdleObserver(home, workspace) as observer:
                    self.assertEqual([], observer.foreign_jobs('mine'))
                    database.rename(database.with_suffix('.old'))
                    if replace:
                        replacement = sqlite3.connect(database)
                        replacement.execute('CREATE TABLE jobs(id TEXT,task_id TEXT,state TEXT)')
                        replacement.close()
                    with self.assertRaises(sqlite3.OperationalError):
                        observer.foreign_jobs('mine')

    def test_symlink_store_and_workspace_retargeting_fail_closed(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            workspace = home / 'workspace'
            workspace.mkdir()
            database, writer = self.database(home, workspace)
            writer.close()
            real_database = database.with_suffix('.real')
            database.rename(real_database)
            database.symlink_to(real_database)
            with self.assertRaises(sqlite3.OperationalError):
                with service_personal.JobIdleObserver(home, workspace):
                    self.fail('a symlink store must not be observed')
            database.unlink()
            real_database.rename(database)
            alias = home / 'workspace-alias'
            alias.symlink_to(workspace)
            another_workspace = home / 'another-workspace'
            another_workspace.mkdir()
            with service_personal.JobIdleObserver(home, alias) as observer:
                alias.unlink()
                alias.symlink_to(another_workspace)
                with self.assertRaises(sqlite3.OperationalError):
                    observer.foreign_jobs('mine')

    def test_missing_store_is_not_created_and_failed_entry_closes_descriptor(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            observer = service_personal.JobIdleObserver(home, home)
            with self.assertRaises(sqlite3.OperationalError):
                with observer:
                    self.fail('a missing store cannot be idle evidence')
            self.assertFalse(observer.database.exists())
            self.assertIsNone(observer.connection)
            self.assertIsNone(observer.descriptor)

    def test_sqlite_read_failure_preserves_service_and_closes_observer(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            workspace = home / 'workspace'
            workspace.mkdir()
            database, writer = self.database(home, workspace)
            writer.execute('PRAGMA journal_mode=WAL').fetchone()
            restarts = []
            observer = service_personal.JobIdleObserver(home, workspace)
            try:
                with observer:
                    with patch.object(restart_personal.time, 'sleep'):
                        try:
                            # Some SQLite builds cannot initialize an ordinary
                            # RO WAL read before a writer creates the sidecars.
                            observer.foreign_jobs('mine')
                        except sqlite3.OperationalError:
                            with self.assertRaises(sqlite3.OperationalError):
                                restart_personal.guarded_restart(42, lambda: 42,
                                    lambda: not observer.foreign_jobs('mine'), lambda: restarts.append(True))
                        else:
                            # Builds that support this case must still preserve
                            # the listener after the selected file disappears.
                            database.rename(database.with_suffix('.missing'))
                            with self.assertRaises(sqlite3.OperationalError):
                                restart_personal.guarded_restart(42, lambda: 42,
                                    lambda: not observer.foreign_jobs('mine'), lambda: restarts.append(True))
                self.assertEqual([], restarts)
                self.assertIsNone(observer.connection)
                self.assertIsNone(observer.descriptor)
            finally:
                writer.close()


if __name__ == '__main__':
    unittest.main()
