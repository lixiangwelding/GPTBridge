"""Offline file-handoff regressions; no Windows UI, network or sender is invoked."""
import fcntl
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts/chatgpt-tasks'))
from scheduler import atomic_json, digest
from submit_queue import submit


class SupersessionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.workspace = self.root / 'workspace'; self.workspace.mkdir()
        self.queue = self.root / 'queue'; (self.queue / 'inbox').mkdir(parents=True)
        self.plan = self.root / 'plan.json'
        atomic_json(self.plan, dict(connector='GPTBridge Windows', title='three replacement tasks',
            tasks=[dict(id='A',title='A',dependsOn=[],customPrompt='replacement work')]))
        self.old = self.queue / 'inbox/old.json'
        atomic_json(self.old, dict(schemaVersion=1,connector='GPTBridge Windows',batchId='old',
            workspace=str(self.workspace),tasks=[dict(id='T01',prompt='old work',dependsOn=[])]))
        self.state = self.queue / 'states/old.json'
        atomic_json(self.state,dict(batchId='old',inputSha256=digest(self.old),status='running',
            tasks=dict(T01=dict(status='waiting_client',requestId='nonce1',nonce='nonce1',receiptPath='out/result.json'))))
        self.audit = self.queue / 'attempts/nonce1.stage.desktop.json'
        atomic_json(self.audit,dict(requestId='nonce1',status='busy'))
        atomic_json(self.queue / 'scheduler-status.json',dict(status='stopped'))
        self.before = {p:p.read_bytes() for p in [self.old,self.state,self.audit]}

    def replace(self, new_id='new'):
        return submit(self.plan,self.root,self.workspace,self.queue,new_id,supersedes='old')

    def assert_old_retained(self):
        self.assertEqual(self.state.read_bytes(),self.before[self.state])
        self.assertEqual(self.audit.read_bytes(),self.before[self.audit])
        self.assertEqual((self.queue/'superseded/old/batch.json').read_bytes(),self.before[self.old])

    def test_replacement_keeps_original_audits_and_is_idempotent(self):
        self.assertTrue(self.replace()['oldBatchArchived'])
        self.assertFalse(self.old.exists())
        self.assertEqual([p.stem for p in (self.queue/'inbox').glob('*.json')],['new'])
        self.assert_old_retained()
        self.assertTrue(self.replace()['duplicate'])
        with self.assertRaises(ValueError):self.replace('different')
        self.assert_old_retained()

    def test_uncertain_or_prepared_rows_cannot_be_replaced(self):
        for status in ['unknown','sent','preparing','prepared','completed']:
            with self.subTest(status=status):
                state=json.loads(self.before[self.state]);state['tasks']['T01']['status']=status
                atomic_json(self.state,state)
                with self.assertRaises(ValueError):self.replace()
                self.assertEqual(self.old.read_bytes(),self.before[self.old])
                self.assertFalse((self.queue/'inbox/new.json').exists())

    def test_send_markers_and_receipts_refuse_supersession(self):
        for evidence in [self.queue/'attempts/nonce1.send.desktop.json',self.workspace/'out/result.json']:
            with self.subTest(path=evidence):
                atomic_json(evidence,{})
                with self.assertRaises(ValueError):self.replace()
                self.assertTrue(self.old.exists()); evidence.unlink()

    def test_unknown_missing_or_wrong_preparation_audit_refuses(self):
        for result in [dict(status='unknown',requestId='nonce1'),dict(status='busy',requestId='other'),None]:
            with self.subTest(result=result):
                if result is None:self.audit.unlink()
                else:atomic_json(self.audit,result)
                with self.assertRaises((ValueError,FileNotFoundError)):self.replace()
                self.assertTrue(self.old.exists())

    def test_live_owner_or_enabled_marker_refuses(self):
        with (self.queue/'owner.lock').open('a+') as owner:
            fcntl.flock(owner,fcntl.LOCK_EX|fcntl.LOCK_NB)
            with self.assertRaises(ValueError):self.replace()
        (self.queue/'auto-enabled').touch()
        with self.assertRaises(ValueError):self.replace()
        self.assertTrue(self.old.exists())

    def test_failure_before_publish_restores_old_input(self):
        with patch('submit_queue.publish_batch',side_effect=OSError('fixture disk full')):
            with self.assertRaises(OSError):self.replace()
        self.assertEqual(self.old.read_bytes(),self.before[self.old])
        self.assertFalse((self.queue/'inbox/new.json').exists())
        self.assertTrue(self.replace()['submitted'])
        self.assert_old_retained()

    def test_interruption_after_publish_never_restores_old_active_input(self):
        from submit_queue import publish_batch
        def interrupted(batch,queue):
            publish_batch(batch,queue)
            raise KeyboardInterrupt()
        with patch('submit_queue.publish_batch',side_effect=interrupted):
            with self.assertRaises(KeyboardInterrupt):self.replace()
        self.assertFalse(self.old.exists())
        self.assertTrue((self.queue/'inbox/new.json').exists())
        self.assertTrue(self.replace()['duplicate'])
        self.assert_old_retained()

    def test_invalid_new_plan_or_changed_old_input_is_rejected(self):
        plan=json.loads(self.plan.read_text());plan['tasks'][0]['dependsOn']=['missing'];atomic_json(self.plan,plan)
        with self.assertRaises(ValueError):self.replace()
        self.assertTrue(self.old.exists())
        self.assertFalse((self.queue/'superseded').exists())
        plan['tasks'][0]['dependsOn']=[];atomic_json(self.plan,plan)
        self.old.write_text(self.old.read_text()+' ')
        with self.assertRaises(ValueError):self.replace()

    def test_status_distinguishes_active_input_from_archived_history(self):
        from submit_queue import main
        self.replace()
        output=io.StringIO()
        with patch.object(sys,'argv',['submit_queue.py','status','--queue',str(self.queue)]),contextlib.redirect_stdout(output):
            main()
        status=json.loads(output.getvalue())
        self.assertEqual(status['scheduler']['status'],'stopped')
        self.assertEqual([b['batchId'] for b in status['pending']],['new'])
        self.assertEqual(status['pending'][0]['taskIds'],['A'])
        self.assertEqual(status['superseded'][0]['oldBatchId'],'old')
        self.assertEqual(status['batches'][0]['batchId'],'old')


if __name__ == '__main__':unittest.main()
