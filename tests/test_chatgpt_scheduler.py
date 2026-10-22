import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts/chatgpt-tasks'))
from scheduler import Engine,atomic_json,digest,inside,verify_receipt,validate_batch
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


if __name__=='__main__':unittest.main()
