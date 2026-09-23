#!/usr/bin/env python3
"""Exact, hash-gated release steps; no installation or production service changes."""
from __future__ import annotations
import argparse
import datetime
import hashlib
import importlib.util
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOC = pathlib.Path(__file__).resolve().parent
REL = DOC.relative_to(ROOT).as_posix()
BASE = 'ecc4452a1bb4a91497f8f24e9baeda3831b683e2'
VALIDATION = ROOT / '.artifacts/continuation-193c2d12/verify-1790172574847283000'
TRACKED = '''PERSONAL.md
README.en.md
README.md
desktop-plugin/.app.json
desktop-plugin/.codex-plugin/plugin.json
desktop-plugin/README.md
desktop-plugin/skills/codex-infinite/SKILL.md
package-lock.json
package.json
scripts/desktop_personal.py
src-tauri/Cargo.lock
src-tauri/Cargo.toml
src-tauri/src/actions/openapi.rs
src-tauri/src/commands/taskdock.rs
src-tauri/src/commands/workspace.rs
src-tauri/src/lib.rs
src-tauri/src/tools/audit_regression_tests.rs
src-tauri/src/tools/dispatch.rs
src-tauri/src/tools/mod.rs
src-tauri/src/tools/patch.rs
src-tauri/src/tools/personal_patch.rs
src-tauri/src/tools/policy.rs
src-tauri/src/tools/registry.rs
src-tauri/src/tools/skills.rs
src-tauri/src/workspace/mod.rs
src-tauri/src/workspace/model.rs
src-tauri/tauri.conf.json
src-tauri/tests/call_tool_contract.rs
src-tauri/tests/desktop_native.rs
src/app.html
src/lib/app-links.ts
src/lib/components/CloseConfirmDialog.svelte
src/lib/taskdock/ConnectionDialog.svelte
src/lib/taskdock/ProjectDialog.svelte
src/lib/taskdock/Shell.svelte
src/lib/taskdock/TaskInspector.svelte
src/lib/taskdock/Workbench.svelte
src/lib/taskdock/api.ts
src/lib/taskdock/taskdock.css
src/lib/types.ts
src/routes/projects/+page.svelte
src/routes/settings/+page.svelte
src/routes/skills/+page.svelte
src/routes/workspace/[id]/+page.svelte
tests/personal/test_desktop_personal.py'''.splitlines()
NEW = ['desktop-plugin/skills/gptbridge-plugin/SKILL.md', 'src-tauri/src/tools/skill_write.rs',
       'src/lib/components/SkillWriteRootsForm.svelte', 'tests/gptbridge-brand.test.mjs',
       'tests/skill-write-roots-form.test.mjs', 'docs/gptbridge/BRANDING.md',
       'docs/gptbridge/SCREENSHOTS.md', 'docs/gptbridge/VALIDATION.md']
NEW += [f'docs/gptbridge/screenshots/browser-preview-0.4.1/{page}-{size}.png'
        for page in ('workbench','projects','skills','settings') for size in ('desktop','mobile')]
NEW += ['docs/gptbridge/screenshots/browser-preview-0.4.1/capture-manifest.json',
        'docs/gptbridge/screenshots/browser-preview-0.4.1/source-manifest.json']
NEW += [f'{REL}/{name}' for name in ('validate.py','release.py','REVIEW.md','qualified-results.json','qualified-source.json')]
FILES = sorted(TRACKED + NEW)

def git(*args: str, timeout: int = 25) -> bytes:
    result = subprocess.run(['git','--no-optional-locks',*args],cwd=ROOT,capture_output=True,timeout=timeout)
    if result.returncode:
        # Do not print remote URLs or arbitrary authentication stderr.
        raise RuntimeError(f'git {args[0]} failed with exit {result.returncode}')
    return result.stdout

def save(name: str, value: object) -> None:
    with (DOC/name).open('x',encoding='utf-8') as out:
        json.dump(value,out,ensure_ascii=False,indent=2); out.write('\n')

def current_hash(name: str) -> str | None:
    path = ROOT/name
    if path.is_symlink(): raise RuntimeError('Source symlink: '+name)
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None

def verify_source() -> None:
    spec=importlib.util.spec_from_file_location('candidate_validation',DOC/'validate.py')
    module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    expected=json.loads((VALIDATION/'source-after.json').read_text())
    current=module.fingerprint()
    # Staging a verified deletion removes it from git ls-files. Its physical
    # absence remains the same qualified source state, not a new source drift.
    for name,value in expected.items():
        if value is None and name not in current and not (ROOT/name).exists():
            current[name]=None
    if current!=expected:
        raise RuntimeError('Source differs from qualified tests: '+', '.join(sorted(k for k in current.keys()|expected.keys() if current.get(k)!=expected.get(k))))

def head() -> str: return git('rev-parse','HEAD').decode().strip()

def verify_manifest() -> dict:
    manifest=json.loads((DOC/'candidate-manifest.json').read_text())
    verify_source()
    for name,digest in manifest['files'].items():
        if current_hash(name)!=digest: raise RuntimeError('Candidate changed: '+name)
    return manifest

def verify_index(manifest: dict) -> None:
    staged=set(filter(None,git('diff','--cached','--no-renames','--name-only','-z').decode().split('\0')))
    if staged!=set(manifest['files']): raise RuntimeError('Staged file set changed')
    for name,digest in manifest['files'].items():
        if digest is None:
            if git('ls-files','--stage','--',name): raise RuntimeError('Expected deletion absent from index: '+name)
        elif hashlib.sha256(git('show',':'+name)).hexdigest()!=digest:
            raise RuntimeError('Staged bytes changed: '+name)
    git('diff','--cached','--check')

def main() -> None:
    parser=argparse.ArgumentParser(); parser.add_argument('step',choices=['prepare','stage','commit','push']); step=parser.parse_args().step
    if git('branch','--show-current').decode().strip()!='main': raise RuntimeError('Expected independent main branch')
    if step=='prepare':
        if head()!=BASE: raise RuntimeError('HEAD advanced; reconcile before preparing')
        if git('diff','--cached','--name-only'): raise RuntimeError('Index is not empty')
        result=json.loads((VALIDATION/'result.json').read_text())
        if result.get('passed') is not True or result.get('source_drift'): raise RuntimeError('Validation is not passed')
        verify_source()
        save('qualified-results.json',result)
        save('qualified-source.json',json.loads((VALIDATION/'source-after.json').read_text()))
        assert len(FILES)==len(set(FILES))
        hashes={name:current_hash(name) for name in FILES}
        missing=[name for name,value in hashes.items() if value is None and name!='desktop-plugin/skills/codex-infinite/SKILL.md']
        if missing: raise RuntimeError('Missing candidate paths: '+', '.join(missing))
        manifest={'created_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'baseline_head':BASE,
                  'scope':'GPTBridge0.4.1 brand/plugin + approved-root implementation + source-qualified continuation evidence',
                  'count':len(hashes),'files':hashes,'validation':str(VALIDATION/'result.json'),
                  'excluded':'All unrelated logs, nested build caches, AI-OPS, memory goals and historical handoff/FRP task documents',
                  'installation_performed':False}
        save('candidate-manifest.json',manifest)
        print(json.dumps(manifest,ensure_ascii=False,indent=2));return
    manifest=verify_manifest()
    if step=='stage':
        if head()!=BASE or git('diff','--cached','--name-only'): raise RuntimeError('HEAD/index no longer matches preparation')
        git('add','--',*FILES)
        verify_index(manifest)
        save('staged-receipt.json',{'head':BASE,'count':len(FILES),'staged_tree':git('write-tree').decode().strip(),
                                   'diff_sha256':hashlib.sha256(git('diff','--cached','--binary')).hexdigest()})
        print('STAGED_EXACT',len(FILES));return
    if step=='commit':
        if head()!=BASE: raise RuntimeError('HEAD advanced')
        verify_index(manifest)
        approval=json.loads((DOC/'review-approval.json').read_text())
        if approval.get('approved_for_source_commit') is not True: raise RuntimeError('Review has not approved the exact staged scope')
        if approval.get('staged_tree')!=git('write-tree').decode().strip(): raise RuntimeError('Reviewed tree differs from staged tree')
        git('commit','-m','feat(gptbridge): 完成0.4.1品牌与受控Skill写根交付',timeout=90)
        commit=head()
        if git('rev-parse',commit+'^').decode().strip()!=BASE: raise RuntimeError('Unexpected parent after commit')
        if set(filter(None,git('diff-tree','--no-commit-id','--no-renames','--name-only','-r','-z',commit).decode().split('\0')))!=set(FILES):
            raise RuntimeError('Unexpected committed scope')
        save('commit-receipt.json',{'commit':commit,'parent':BASE,'files':len(FILES),'push_performed':False})
        print('COMMITTED',commit);return
    receipt=json.loads((DOC/'commit-receipt.json').read_text());commit=receipt['commit']
    if head()!=commit: raise RuntimeError('HEAD advanced; inspect before push')
    git('push','origin','HEAD:refs/heads/main',timeout=90)
    remote=git('ls-remote','origin','refs/heads/main',timeout=30).decode().split()[0]
    if remote!=commit: raise RuntimeError('Remote advanced; inspect ancestry before claiming verification')
    save('push-receipt.json',{'commit':commit,'remote_main':remote,'remote_readback_equal':True,
                            'completed_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
                            'force_push':False,'installation_performed':False})
    print('PUSHED_AND_VERIFIED',commit)

if __name__=='__main__':main()
