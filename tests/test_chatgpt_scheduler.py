import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts/chatgpt-tasks'))
from scheduler import Engine,WindowsAdapter,atomic_json,digest,inside,verify_receipt,validate_batch,diagnostic_event
from unittest.mock import patch
import subprocess
from submit_queue import submit


class SchedulerTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.workspace=self.root/'workspace';self.workspace.mkdir();self.queue=self.root/'queue';(self.queue/'inbox').mkdir(parents=True)
        self.batch=dict(schemaVersion=1,connector='GPTBridge Windows',batchId='batch1',workspace=str(self.workspace),tasks=[dict(id='T01',dependsOn=[],prompt='first'),dict(id='T02',dependsOn=['T01'],prompt='second')])
        atomic_json(self.queue/'inbox/batch1.json',self.batch);self.calls=[]
        def adapter(req):
            self.calls.append(dict(req));return dict(status='prepared' if req['action']=='stage' else 'sent',windowRef='fixture:1',detail='OFFLINE_FIXTURE_ONLY')
        self.adapter=adapter;self.engine=Engine(self.queue,self.workspace,adapter,dict(client='desktop',model='Pro',effort='深入思考'))
    def tearDown(self):self.temp.cleanup()
    def state(self):return json.loads((self.queue/'states/batch1.json').read_text(encoding='utf-8'))
    def receipt(self,task='T01',nonce=None):
        row=self.state()['tasks'][task];a=self.workspace/(task+'.txt');a.write_text('fixture artifact');log=self.workspace/(task+'.log');log.write_text('fixture check exit0')
        r=dict(schemaVersion=1,batchId='batch1',taskId=task,nonce=nonce or row['nonce'],status='completed',summary='fixture only',artifacts=[dict(path=a.name,sha256=digest(a))],checks=[dict(command='fixture check',exitCode=0,logPath=log.name,logSha256=digest(log))])
        atomic_json(self.workspace/row['receiptPath'],r);return r
    def test_sequential_progress_requires_hashed_receipt(self):
        self.assertEqual(self.engine.tick(),'sent');self.assertEqual([x['action'] for x in self.calls],['stage','send'])
        self.assertEqual(self.engine.tick(),'waiting_receipt');self.assertEqual(len(self.calls),2)
        self.receipt();self.assertEqual(self.engine.tick(),'sent');self.assertEqual(self.calls[-1]['taskId'],'T02')
        self.receipt('T02');self.assertEqual(self.engine.tick(),'completed')
    def test_restart_after_unknown_never_replays(self):
        def fail(req):
            self.calls.append(req)
            if req['action']=='send':raise RuntimeError('fixture disconnect')
            return dict(status='prepared',windowRef='fixture:1')
        self.engine.adapter=fail
        with self.assertRaises(RuntimeError):self.engine.tick()
        self.assertEqual(self.state()['tasks']['T01']['status'],'unknown')
        restarted=Engine(self.queue,self.workspace,self.adapter,self.engine.options)
        (self.queue/'retry-preparation').touch()
        self.assertEqual(restarted.tick(),'waiting_receipt');self.assertEqual(len(self.calls),2)
        self.receipt();self.assertEqual(restarted.tick(),'sent');self.assertEqual(self.calls[-1]['taskId'],'T02')
    def test_pause_after_fill_resumes_same_unsent_request(self):
        def pause(req):
            self.calls.append(dict(req))
            if req['action']=='stage':
                (self.queue/'pause').touch()
                return dict(status='prepared',windowRef='fixture:1')
            return dict(status='sent')
        self.engine.adapter=pause
        self.assertEqual(self.engine.tick(),'paused_before_send')
        self.assertEqual([x['action'] for x in self.calls],['stage'])
        (self.queue/'pause').unlink()
        self.assertEqual(self.engine.tick(),'sent')
        self.assertEqual([x['action'] for x in self.calls],['stage','send'])
        self.assertEqual(self.calls[0]['requestId'],self.calls[1]['requestId'])

    def test_bad_receipt_and_changed_output_never_advance(self):
        self.engine.tick();self.receipt(nonce='old-nonce')
        with self.assertRaises(ValueError):self.engine.tick()
        self.receipt();(self.workspace/'T01.txt').write_text('changed')
        with self.assertRaises(ValueError):self.engine.tick()
        self.assertEqual(len(self.calls),2)
    def test_late_receipt_recovers_timeout_without_redispatch(self):
        self.engine.tick();state=self.state();state['tasks']['T01']['startedAt']=0;atomic_json(self.queue/'states/batch1.json',state)
        self.assertEqual(self.engine.tick(),'attention');self.assertEqual(self.engine.tick(),'attention');self.assertEqual(len(self.calls),2)
        self.receipt();self.assertEqual(self.engine.tick(),'sent');self.assertEqual(self.calls[-1]['taskId'],'T02')
        self.assertEqual([c['taskId'] for c in self.calls],['T01','T01','T02','T02'])

    def test_explicit_blocker_only_stops_its_dependents(self):
        self.batch['tasks'].append(dict(id='T03',dependsOn=['T01'],prompt='independent third'))
        self.batch['tasks'].append(dict(id='T04',dependsOn=['T02'],prompt='waits for blocked second'))
        atomic_json(self.queue/'inbox/batch1.json',self.batch)
        self.engine.tick();self.receipt();self.engine.tick()
        row=self.state()['tasks']['T02']
        atomic_json(self.workspace/row['receiptPath'],dict(schemaVersion=1,batchId='batch1',taskId='T02',nonce=row['nonce'],status='blocked',summary='fixture missing input'))
        self.assertEqual(self.engine.tick(),'sent');self.assertEqual(self.calls[-1]['taskId'],'T03')
        self.receipt('T03');self.assertEqual(self.engine.tick(),'blocked')
        self.assertNotIn('T04',self.state()['tasks']);self.assertEqual(self.state()['tasks']['T02']['status'],'blocked')

    def test_busy_client_never_interrupts_or_changes_request(self):
        def busy(req):
            self.calls.append(dict(req))
            return dict(status='busy',detail='fixture busy; no input sent')
        self.engine.adapter=busy
        self.assertEqual(self.engine.tick(),'waiting_client');self.assertEqual(self.engine.tick(),'waiting_client')
        self.assertEqual(self.calls[0]['requestId'],self.calls[1]['requestId'])
        self.assertTrue(all(c['action']=='stage' for c in self.calls))
        self.engine.adapter=self.adapter;self.assertEqual(self.engine.tick(),'sent')
        self.assertEqual(self.calls[0]['requestId'],self.calls[-1]['requestId'])

    def test_fallback_only_before_send(self):
        def adapter(req):
            self.calls.append(dict(req))
            if req['client']=='desktop':return dict(status='blocked',detail='no desktop')
            return dict(status='prepared' if req['action']=='stage' else 'unknown',windowRef='fixture-web')
        self.engine.adapter=adapter;self.engine.options['webFallback']=True
        self.assertEqual(self.engine.tick(),'unknown');self.assertEqual([x['client'] for x in self.calls],['desktop','web','web'])
        self.engine.tick();self.assertEqual(len(self.calls),3)

    def activation_wait(self, req):
        self.calls.append(dict(req))
        result=dict(status='busy',reasonCode='foreground_unavailable',requestId=req['requestId'],detail='fixture activation not confirmed')
        atomic_json(self.queue/'attempts'/(req['requestId']+'.stage.desktop.json'),result)
        return result

    def test_foreground_retry_backs_off_without_fallback_or_new_request(self):
        self.engine.adapter=self.activation_wait;self.engine.options['webFallback']=True
        with patch('scheduler.time.time',return_value=1000):
            self.assertEqual(self.engine.tick(),'waiting_client')
            self.assertEqual(self.engine.tick(),'waiting_client')
        self.assertEqual(len(self.calls),1)
        row=self.state()['tasks']['T01'];self.assertEqual(row['activationRetry'],dict(count=1,nextAttemptAt=1005))
        self.assertEqual(self.calls[0]['client'],'desktop')
        restarted=Engine(self.queue,self.workspace,self.activation_wait,self.engine.options)
        with patch('scheduler.time.time',return_value=1005):self.assertEqual(restarted.tick(),'waiting_client')
        self.assertEqual(self.state()['tasks']['T01']['activationRetry']['count'],2)
        self.assertEqual(self.calls[0]['requestId'],self.calls[1]['requestId'])
        self.engine.adapter=self.adapter
        with patch('scheduler.time.time',return_value=1015):self.assertEqual(self.engine.tick(),'sent')
        self.assertEqual(self.calls[0]['requestId'],self.calls[-1]['requestId'])
        self.assertEqual([c['action'] for c in self.calls],['stage','stage','stage','send'])

    def test_foreground_retry_exhausts_and_requires_owner_resume(self):
        self.engine.adapter=self.activation_wait
        for i in range(12):
            with patch('scheduler.time.time',return_value=1000+i*31):
                self.assertEqual(self.engine.tick(),'blocked' if i==11 else 'waiting_client')
        self.assertEqual(len(self.calls),12)
        self.assertEqual(self.state()['tasks']['T01']['activationRetry']['count'],12)
        self.assertEqual(len({c['requestId'] for c in self.calls}),1)
        self.assertEqual(self.engine.tick(),'blocked');self.assertEqual(len(self.calls),12)
        (self.queue/'retry-preparation').touch();self.engine.adapter=self.adapter
        self.assertEqual(self.engine.tick(),'sent')
        self.assertEqual(self.calls[0]['requestId'],self.calls[-1]['requestId'])
        self.assertNotIn('activationRetry',self.state()['tasks']['T01'])

    def test_activation_resume_rejects_unknown_busy_and_any_send_marker(self):
        self.engine.adapter=self.activation_wait
        self.engine.tick();baseline=self.state();baseline.update(status='blocked');baseline['tasks']['T01']['status']='blocked'
        rid=baseline['tasks']['T01']['requestId'];audit=self.queue/'attempts'/(rid+'.stage.desktop.json')
        good=json.loads(audit.read_text())
        for condition in ['generic_busy','wrong_request','unknown','send_marker']:
            with self.subTest(condition=condition):
                atomic_json(self.queue/'states/batch1.json',baseline);result=dict(good)
                if condition=='generic_busy':result.pop('reasonCode')
                if condition=='wrong_request':result['requestId']='foreign'
                if condition=='unknown':result['status']='unknown'
                atomic_json(audit,result)
                send=self.queue/'attempts'/(rid+'.send.desktop.json')
                if condition=='send_marker':send.touch()
                (self.queue/'retry-preparation').touch();self.engine.adapter=self.adapter
                self.assertEqual(self.engine.tick(),'blocked');self.assertEqual(len(self.calls),1)
                send.unlink(missing_ok=True)

    @unittest.skipUnless(sys.platform=='win32', 'Windows PowerShell offline fixtures')
    def test_windows_offline_mode_and_foreground_fixtures(self):
        # The existing Windows CI discovers this test. All OS/window callbacks are
        # injected in the fixture; it never discovers or operates live UI.
        result=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass',
                               '-File',str(ROOT/'tests/chatgpt-mode-gate.test.ps1')],
                              capture_output=True,timeout=90)
        self.assertEqual(result.returncode,0,(result.stdout+result.stderr).decode('utf-8',errors='replace'))
        self.assertIn(b'FOREGROUND_FIXTURE_CASES_PASSED=9',result.stdout)
        self.assertIn(b'MODE_CASES_PASSED=19',result.stdout)
        self.assertIn(b'WORK_STOP_ORIGIN_CASES_PASSED=11',result.stdout)
        self.assertIn(b'MENTION_APPEND_RANGE_CASES_PASSED=3',result.stdout)
        self.assertIn(b'PURE_CHAT_MODE_CASES_PASSED=6',result.stdout)
        self.assertIn(b'RICH_COMPOSER_CASES_PASSED=6',result.stdout)
        self.assertIn(b'SENDINPUT_NOT_CALLED',result.stdout)

    def block_preparation(self):
        def blocked(req):
            self.calls.append(dict(req))
            result=dict(status='blocked',requestId=req['requestId'],detail='fixture missing composer')
            atomic_json(self.queue/'attempts'/(req['requestId']+'.stage.'+req['client']+'.json'),result)
            return result
        self.engine.adapter=blocked
        self.assertEqual(self.engine.tick(),'blocked')
        return self.state()['tasks']['T01']

    def test_owner_resume_recovers_only_proven_unsent_same_request(self):
        row=self.block_preparation();original_nonce=row['nonce']
        self.assertEqual(self.engine.tick(),'blocked');self.assertEqual(len(self.calls),1)
        self.engine.adapter=self.adapter
        (self.queue/'retry-preparation').touch()
        self.assertEqual(self.engine.tick(),'sent')
        self.assertEqual([c['action'] for c in self.calls],['stage','stage','send'])
        self.assertTrue(all(c['requestId']==original_nonce for c in self.calls))
        self.assertEqual(len(self.state()['tasks']['T01']['preparationRecovery']),1)
        self.assertFalse((self.queue/'retry-preparation').exists())
        self.assertEqual(self.engine.tick(),'waiting_receipt');self.assertEqual(len(self.calls),3)

    def test_owner_resume_rejects_missing_unknown_or_mismatched_audits_and_any_send_marker(self):
        row=self.block_preparation();baseline=self.state();rid=row['requestId']
        audit=self.queue/'attempts'/(rid+'.stage.desktop.json')
        good=json.loads(audit.read_text());send=self.queue/'attempts'/(rid+'.send.web.input.json')
        for condition in ['missing','unknown','prepared','wrong_request','send_marker']:
            with self.subTest(condition=condition):
                atomic_json(self.queue/'states/batch1.json',baseline)
                result=dict(good)
                if condition=='missing':audit.unlink(missing_ok=True)
                else:
                    if condition in ('unknown','prepared'):result['status']=condition
                    if condition=='wrong_request':result['requestId']='another'
                    atomic_json(audit,result)
                if condition=='send_marker':send.touch()
                (self.queue/'retry-preparation').touch();self.engine.adapter=self.adapter
                self.assertEqual(self.engine.tick(),'blocked');self.assertEqual(len(self.calls),1)
                self.assertEqual(self.state()['tasks']['T01']['nonce'],rid)
                send.unlink(missing_ok=True)

    def test_owner_resume_preserves_business_blocked_receipt(self):
        row=self.block_preparation()
        atomic_json(self.workspace/row['receiptPath'],dict(schemaVersion=1,batchId='batch1',taskId='T01',nonce=row['nonce'],status='blocked',summary='business input missing'))
        (self.queue/'retry-preparation').touch();self.engine.adapter=self.adapter
        self.assertEqual(self.engine.tick(),'blocked');self.assertEqual(len(self.calls),1)

    def test_unknown_preparation_owner_confirmation_queries_then_continues_same_chat(self):
        row=self.block_preparation();rid=row['nonce'];audit=self.queue/'attempts'/(rid+'.stage.desktop.json')
        atomic_json(audit,dict(status='unknown',requestId=rid,windowRef='',detail='fixture exception'))
        confirmed=dict(requestId=rid,auditSha256=digest(audit))
        def query(req):
            self.calls.append(dict(req))
            if req['action']=='query_preparation':
                self.assertFalse(req['newChat'])
                return dict(status='ready',reasonCode='unsent_mention_only',requestId=rid,windowRef='fixture:1')
            if req['action']=='stage':
                self.assertFalse(req['newChat']);self.assertEqual(req['windowRef'],'fixture:1')
                return dict(status='prepared',windowRef='fixture:1')
            return dict(status='sent')
        self.engine.adapter=query
        atomic_json(self.queue/'retry-preparation',dict(confirmedUnsent=[confirmed]))
        self.assertEqual(self.engine.tick(),'sent')
        self.assertEqual([c['action'] for c in self.calls],['stage','query_preparation','stage','send'])
        final=self.state()['tasks']['T01'];self.assertEqual(final['nonce'],rid)
        self.assertEqual(final['preparationRecovery'][-1]['audits'][0]['result']['status'],'unknown')

    def test_unknown_preparation_requires_bound_confirmation_and_readonly_query(self):
        row=self.block_preparation();rid=row['nonce'];audit=self.queue/'attempts'/(rid+'.stage.desktop.json')
        atomic_json(audit,dict(status='unknown',requestId=rid,windowRef='fixture:1'))
        baseline=self.state()
        for condition in ['bad_hash','query_blocked','query_wrong_request','window_changed','send_marker']:
            with self.subTest(condition=condition):
                atomic_json(self.queue/'states/batch1.json',baseline)
                confirmed=dict(requestId=rid,auditSha256='bad' if condition=='bad_hash' else digest(audit))
                send=self.queue/'attempts'/(rid+'.send.desktop.json')
                if condition=='send_marker':send.touch()
                def query(req):
                    self.assertEqual(req['action'],'query_preparation');self.calls.append(dict(req))
                    return dict(status='blocked' if condition=='query_blocked' else 'ready',reasonCode='unsent_mention_only',
                                requestId='foreign' if condition=='query_wrong_request' else rid,
                                windowRef='changed' if condition=='window_changed' else 'fixture:1')
                self.engine.adapter=query;before=len(self.calls)
                atomic_json(self.queue/'retry-preparation',dict(confirmedUnsent=[confirmed]))
                self.assertEqual(self.engine.tick(),'blocked')
                self.assertLessEqual(len(self.calls)-before,1)
                self.assertEqual(self.state()['tasks']['T01']['nonce'],rid)
                send.unlink(missing_ok=True)

    def test_preparation_fallback_keeps_both_failure_reasons(self):
        self.engine.options['webFallback']=True;self.block_preparation()
        attempts=self.state()['tasks']['T01']['preparationAttempts']
        self.assertEqual([a['client'] for a in attempts],['desktop','web'])
        self.assertTrue(all(a['status']=='blocked' and a['detail']=='fixture missing composer' for a in attempts))
    def test_pause_stop_and_input_tamper(self):
        (self.queue/'pause').touch();self.assertEqual(self.engine.tick(),'paused');self.assertFalse(self.calls)
        (self.queue/'pause').unlink();self.engine.tick();self.batch['tasks'][0]['prompt']='tampered';atomic_json(self.queue/'inbox/batch1.json',self.batch)
        with self.assertRaises(ValueError):self.engine.tick()
        (self.queue/'stop').touch();self.assertEqual(self.engine.tick(),'stopped')
    def test_path_escape_and_forged_zero_rejected(self):
        self.engine.tick();r=self.receipt();r['checks'][0]['exitCode']=False
        with self.assertRaises(ValueError):verify_receipt(self.workspace,r,'batch1','T01',r['nonce'])
        for p in ['../secret','/tmp/file','dir\\file']:
            with self.assertRaises(ValueError):inside(self.workspace,p)
    def test_submission_is_file_only_and_idempotent(self):
        plan=self.root/'plan.json';prompts=self.root/'prompts';prompts.mkdir()
        plan.write_text(json.dumps(dict(title='plan',connector='GPTBridge Windows',tasks=[dict(id='A',title='one',dependsOn=[],status='draft',customPrompt='')])));(prompts/'A.md').write_text('test prompt')
        first=submit(plan,prompts,self.workspace,self.queue,'newbatch');second=submit(plan,prompts,self.workspace,self.queue,'newbatch')
        self.assertFalse(first['dispatched']);self.assertTrue(second['duplicate']);self.assertFalse(self.calls)
    def test_missing_dependency_cycle_and_invalid_connector(self):
        for mutate in [lambda b:b.update(connector='Mac'),lambda b:b['tasks'][0].update(dependsOn=['T02']),lambda b:b['tasks'][0].update(dependsOn=['missing'])]:
            b=json.loads(json.dumps(self.batch));mutate(b)
            with self.assertRaises(ValueError):validate_batch(b)

    def test_diagnostic_metadata_redaction_rotation_and_log_failure(self):
        diagnostic_event(self.queue,'fixture',taskId='T01',prompt='PRIVATE_PROMPT',password='PRIVATE_PASSWORD')
        path=self.queue/'logs/scheduler-events.jsonl'
        record=json.loads(path.read_text())
        self.assertEqual(record['taskId'],'T01');self.assertNotIn('PRIVATE',path.read_text())
        path.write_text('x'*(2*1024*1024))
        diagnostic_event(self.queue,'rotated',status='waiting_client')
        self.assertEqual(json.loads(path.read_text())['event'],'rotated')
        self.assertTrue(path.with_name(path.name+'.1').is_file())
        path.unlink();path.mkdir()
        diagnostic_event(self.queue,'unwritable')  # Does not break dispatch semantics.

    def test_adapter_error_logs_metadata_and_keeps_unknown_attempt(self):
        adapter=WindowsAdapter(self.queue,'unused-fixture.ps1')
        req=dict(taskId='T01',requestId='fixture1',action='stage',client='desktop',prompt='PRIVATE_PROMPT')
        with patch('scheduler.subprocess.check_output',return_value='C:\\fixture.json'),patch('scheduler.subprocess.run',side_effect=subprocess.TimeoutExpired('fixture',50)):
            result=adapter(req)
        self.assertEqual(result['status'],'unknown')
        logs=(self.queue/'logs/scheduler-events.jsonl').read_text()
        self.assertIn('TimeoutExpired',logs);self.assertNotIn('PRIVATE_PROMPT',logs)
        self.assertEqual(json.loads((self.queue/'attempts/fixture1.stage.desktop.json').read_text())['status'],'unknown')
        self.assertFalse(list((self.queue/'attempts').glob('*.input.json')))

    def test_adapter_null_diagnostics_preserves_known_unsent_result(self):
        adapter=WindowsAdapter(self.queue,'unused-fixture.ps1')
        req=dict(taskId='T01',requestId='fixture2',action='stage',client='desktop',prompt='PRIVATE_PROMPT')
        response=dict(status='blocked',detail='no target',requestId='fixture2',diagnostics=None)
        completed=subprocess.CompletedProcess([],0,stdout=json.dumps(response).encode(),stderr=b'')
        with patch('scheduler.subprocess.check_output',return_value='C:\\fixture.json'),patch('scheduler.subprocess.run',return_value=completed):
            result=adapter(req)
        self.assertEqual(result,response)
        record=json.loads((self.queue/'logs/scheduler-events.jsonl').read_text().splitlines()[-1])
        self.assertEqual(record['status'],'blocked');self.assertEqual(record['exitCode'],0)
        response['diagnostics']=dict(targetCount=0,seenDesktopWindows=1,offscreenDesktopWindows=1,desktopRootTypes=['ControlType.Window'],prompt='PRIVATE_PROMPT')
        completed.stdout=json.dumps(response).encode()
        with patch('scheduler.subprocess.check_output',return_value='C:\\fixture.json'),patch('scheduler.subprocess.run',return_value=completed):
            self.assertEqual(adapter(req)['status'],'blocked')
        logs=(self.queue/'logs/scheduler-events.jsonl').read_text()
        record=json.loads(logs.splitlines()[-1]);self.assertEqual(record['targetCount'],0);self.assertEqual(record['offscreenDesktopWindows'],1)
        self.assertNotIn('PRIVATE_PROMPT',logs)


if __name__=='__main__':unittest.main()
