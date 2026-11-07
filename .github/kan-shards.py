"""Execute exact Kan certificate shares and require their complete disjoint union."""
import argparse, hashlib, json, os, re, subprocess
from pathlib import Path
ROOT=Path.cwd()
PLAN=json.loads((ROOT/'.github/kan-shards-plan.json').read_text())
PAYLOAD={'.github/kan-shards-plan.json','.github/kan-shards.py','.github/workflows/kan-certificate-shards.yml'}
TESTS={
 'production':('panproto-inst','reference_kan','exhaustive_kan_hom_profile_pairs_agree_with_the_reference'),
 'reference':('panproto-reference','kan','exhaustive_kan_executes_every_representative_profile_pair'),
}
COUNTERS=['pairs','weighted','sigma_homs','pi_bound4_homs','pi_bound81_homs','sigma_refusals','pi_naturality_refusals','pi_outside_image','pi_over_bound']
def require(value,message):
 if not value:raise ValueError(message)
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def git(*args):return subprocess.check_output(['git',*args],text=True).strip()
def write(p,value):p.write_text(json.dumps(value,indent=2)+'\n')
def identity():
 head=git('rev-parse','HEAD');source=PLAN['source']
 require(git('rev-list','--parents','-n','1',head).split()==[head,source],'wrong harness ancestry')
 require(git('rev-list','--parents','-n','1',source).split()==[source,PLAN['prior_source']],'wrong source ancestry')
 require(set(git('diff','--name-only',source,head).splitlines())==PAYLOAD,'unexpected harness paths')
 require(set(git('diff','--name-only',PLAN['prior_source'],source).splitlines())==set(PLAN['source_files']),'unexpected source changes')
 for p,h in PLAN['source_files'].items():require(sha(ROOT/p)==h,'reviewed test bytes differ')
 require(git('status','--porcelain','--untracked-files=no')=='','tracked checkout differs')
 require(os.environ['GITHUB_SHA']==head,'workflow head differs')
 return {'source':source,'harness':head,'run_id':int(os.environ['GITHUB_RUN_ID']),'attempt':int(os.environ['GITHUB_RUN_ATTEMPT']),'payload_sha256':{p:sha(ROOT/p) for p in sorted(PAYLOAD)}}
def command(kind,operation,test=None):
 package,binary,gate=TESTS[kind]
 args=['cargo','nextest',operation,'--locked','--cargo-profile','ci-test','--profile','default','-p',package,'--test',binary,'-E','test(='+(test or gate)+')']
 return args+(['--message-format','json'] if operation=='list' else ['--no-tests=fail','--no-fail-fast','--test-threads','1','--success-output','immediate','--failure-output','immediate'])
def matches(data):
 return sorted((bid,name) for bid,suite in data['rust-suites'].items() for name,t in suite['testcases'].items() if t['filter-match']['status']=='matches' and not t['ignored'])
def run_one(kind,test,index,out,ident):
 out.mkdir(parents=True,exist_ok=False);record={**ident,'kind':kind,'test':test,'index':index,'complete':False}
 env=os.environ.copy()
 for k in ('PANPROTO_KAN_PAIR_LIMIT','PANPROTO_KAN_PAIR_SHARD','PANPROTO_TEST_SHARD','PANPROTO_EDIT_COVERAGE_SHARD'):env.pop(k,None)
 env['PANPROTO_KAN_THREADS']='4'
 if index is not None:env['PANPROTO_KAN_PAIR_SHARD']=f'{index}/{PLAN["shards"]}'
 record['env']={k:v for k,v in env.items() if k.startswith('PANPROTO_')}
 record['tools']={'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'nextest':subprocess.check_output(['cargo','nextest','--version'],text=True).strip()}
 require(record['tools']['rustc'].startswith('rustc 1.99.0 ') and '0.9.130' in record['tools']['nextest'],'unexpected verification tools')
 try:
  listed=subprocess.run(command(kind,'list',test),env=env,capture_output=True,text=True)
  (out/'list.json').write_text(listed.stdout);(out/'list.stderr').write_text(listed.stderr)
  require(listed.returncode==0,'listing failed');package,binary,_=TESTS[kind]
  require(matches(json.loads(listed.stdout))==[(package+'::'+binary,test)],'selector mismatch')
  record['command']=command(kind,'run',test)
  with (out/'run.log').open('w') as log:
   result=subprocess.run(record['command'],env=env,stdout=log,stderr=subprocess.STDOUT)
  record['exit']=result.returncode;require(result.returncode==0,'test failed');record['complete']=True
 finally:
  record['files_sha256']={str(p.relative_to(out)):sha(p) for p in out.rglob('*') if p.is_file() and p.name!='result.json'};write(out/'result.json',record)
def verify(ident,out):
 subprocess.run(['rustfmt','--edition','2024','--check',*PLAN['source_files']],check=True)
 for kind,(package,binary,_) in TESTS.items():
  subprocess.run(['cargo','clippy','--locked','--profile','ci-test','-p',package,'--test',binary,'--','-D','warnings'],check=True)
  run_one(kind,'representative_pair_shards_match_unsharded_prefix',None,out/kind,ident)
 write(out/'verification.json',{**ident,'accepted':True,'checks':['rustfmt','clippy-both-binaries','both-sharded-prefix-comparisons']})
def aggregate(ident,artifacts,out):
 expected={(kind,index) for kind in TESTS for index in range(PLAN['shards'])};seen={};totals={kind:{key:0 for key in COUNTERS} for kind in TESTS};units={kind:set() for kind in TESTS}
 for path in artifacts.rglob('result.json'):
  row=json.loads(path.read_text());require(all(type(row.get(field)) is int for field in ['index','run_id','attempt']),'noninteger shard identity');key=(row['kind'],row['index']);require(key in expected and key not in seen,'unknown or duplicate shard')
  kind,index=key;require(row['complete'] is True and type(row['exit']) is int and row['exit']==0,'incomplete shard')
  require(all(row.get(k)==v for k,v in ident.items()),'shard identity differs')
  require(row['tools']['rustc'].startswith('rustc 1.99.0 ') and '0.9.130' in row['tools']['nextest'],'unexpected shard tools')
  require(row['test']==TESTS[kind][2] and row['command']==command(kind,'run'),'wrong execution selector')
  require(row['env'].get('PANPROTO_KAN_PAIR_SHARD')==f'{index}/{PLAN["shards"]}' and 'PANPROTO_KAN_PAIR_LIMIT' not in row['env'],'wrong shard environment')
  root=path.parent;actual={str(p.relative_to(root)) for p in root.rglob('*') if p.is_file() and p.name!='result.json'};require(actual==set(row['files_sha256']),'artifact files differ')
  for relative,digest in row['files_sha256'].items():require(sha(root/relative)==digest,'artifact digest differs')
  package,binary,test=TESTS[kind];require(matches(json.loads((root/'list.json').read_text()))==[(package+'::'+binary,test)],'archived selection differs')
  reports=[]
  with (root/'run.log').open() as log:
   for line in log:
    if line.startswith('KAN_PAIR_SHARD '):reports.append(json.loads(line.removeprefix('KAN_PAIR_SHARD ')))
  require(len(reports)==1,'missing or repeated coverage report');r=reports[0]
  require(all(type(r.get(field)) is int for field in ['index','count','total_units']),'noninteger coverage identity')
  require(r['index']==index and r['count']==PLAN['shards'] and r['total_units']==9333,'coverage identity differs')
  require(type(r['units']) is list and all(type(unit) is int for unit in r['units']),'noninteger unit ordinals')
  require(r['units']==list(range(index,9333,PLAN['shards'])),'wrong or repeated unit ordinals')
  require(not units[kind].intersection(r['units']),'overlapping units');units[kind].update(r['units'])
  for field in COUNTERS:require(type(r[field]) is int and r[field]>=0,'invalid counter');totals[kind][field]+=r[field]
  seen[key]={'result_sha256':sha(path),'report':r}
 require(set(seen)==expected,'missing shards')
 for kind,total in totals.items():
  require(units[kind]==set(range(9333)),'incomplete unit union')
  require(total['pairs']==19120495 and total['weighted']==3918714467,'incomplete pair coverage')
  for field in ['sigma_homs','pi_bound4_homs','pi_bound81_homs']+(['sigma_refusals','pi_outside_image','pi_over_bound'] if kind=='reference' else []):require(total[field]>0,'missing required outcome family')
 out.mkdir(exist_ok=False);write(out/'aggregate.json',{**ident,'accepted':True,'scope':'The two complete Kan pair certificates only; other release obligations are separate.','shards':PLAN['shards'],'totals':totals,'results':{f'{k}/{i}':v for (k,i),v in seen.items()}})
def main():
 p=argparse.ArgumentParser();p.add_argument('mode',choices=['verify','run','aggregate']);p.add_argument('--kind',choices=TESTS);p.add_argument('--index',type=int);p.add_argument('--artifacts',type=Path);p.add_argument('--out',type=Path,required=True);a=p.parse_args();ident=identity()
 require(type(PLAN['shards']) is int and PLAN['shards']==32,'unexpected shard count')
 if a.mode=='verify':verify(ident,a.out)
 elif a.mode=='run':
  require(a.kind in TESTS and a.index is not None and 0<=a.index<PLAN['shards'],'invalid share');run_one(a.kind,TESTS[a.kind][2],a.index,a.out,ident)
 else:aggregate(ident,a.artifacts,a.out)
if __name__=='__main__':main()
