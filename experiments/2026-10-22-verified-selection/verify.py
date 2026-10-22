#!/usr/bin/env python3
"""Reproduce scoped approval proofs and verify the standard policy on frozen pages.
No source fetch or model call. --write-registry only records strictly improving subsets.
Rebuild jrg-select after writing, then rerun without the flag to verify activation.
"""
import argparse, hashlib, json, subprocess, sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
OLD = REPO / 'experiments/2026-10-22-web-selection-heldout'
NEW = REPO / 'experiments/2026-10-22-web-selection-rules'
SELECTOR = REPO / 'target/debug/jrg-select'
sys.path.insert(0, str(OLD))
from score import score_units
sys.path.insert(0, str(REPO / 'experiments/2026-10-22-web-selection-eval'))
from evaluate import Support, validate_provenance

def load(p): return json.loads(p.read_text())
def save(p, v): p.write_text(json.dumps(v, ensure_ascii=False, indent=2)+'\n')
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def invoke(request, *args, adapter='documents'):
    return json.loads(subprocess.check_output([str(SELECTOR),'--adapter',adapter,*args,'-'],
        input=json.dumps(request,ensure_ascii=False).encode()))
def docs(path):
    v=load(path)
    return v['documents'] if isinstance(v,dict) else v

def audit_ledger():
    ledger_path=REPO/'data/verified-query-rules.v1.json'
    ledger=load(ledger_path)
    policy=json.loads(subprocess.check_output([str(SELECTOR),'--query-policy-info']))
    assert policy['registry_sha256']==sha(ledger_path)
    for field in ['engine_sha256','catalog_sha256']:
        assert ledger[field]==policy[field]
    assert len(ledger['approvals'])==2
    gold={c['id']:c['units'] for c in load(OLD/'gold.json')['cases']}
    qs={c['id']:c for c in load(OLD/'questions.json')['questions']}
    en={c['id']:c['query_en'] for c in load(OLD/'generated_english.json')}
    for approval in ledger['approvals']:
        evidence=approval['evidence'];path=REPO/evidence['path'];proof=load(path)
        assert path.resolve().is_relative_to(HERE) and sha(path)==evidence['sha256']
        for field in ['request_sha256','rule_ids']:
            assert approval[field]==proof[field]
        for field in ['baseline_units','selected_units']:
            assert evidence[field]==proof[field]
        for field in ['engine_sha256','catalog_sha256']:
            assert proof[field]==policy[field]
        for name,expected in proof['input_files'].items():assert sha(REPO/name)==expected
        for name,expected in proof['outputs'].items():assert sha(HERE/name)==expected
        key=proof['id'];before=load(HERE/'outputs'/f'{key}-off.json');after=load(HERE/'outputs'/f'{key}-subset.json')
        assert score_units(before['documents'],gold[key])==evidence['baseline_units']
        assert score_units(after['documents'],gold[key])==evidence['selected_units']
        assert set(evidence['baseline_units'])<set(evidence['selected_units'])
        request={'query':qs[key]['question_ja'],'query_expansions':[en[key]],'max_tokens':1024,
            'documents':docs(OLD/f'{key}-documents.json')}
        assert invoke(request,'--rule-request-fingerprint')['request_sha256']==approval['request_sha256']
    return {'compiled_registry_matches_file':True,'approvals':len(ledger['approvals']),
        'proof_hashes_sources_outputs_and_gold_checked':True,'policy':policy}

def main(write):
    if not write: audit_ledger()
    (HERE/'outputs').mkdir(exist_ok=True)
    policy=json.loads(subprocess.check_output([str(SELECTOR),'--query-policy-info']))
    support=Support()
    rows, approvals=[],[]
    sources={}
    try:
        qs={c['id']:c for c in load(OLD/'questions.json')['questions']}
        en={c['id']:c['query_en'] for c in load(OLD/'generated_english.json')}
        gold={c['id']:c['units'] for c in load(OLD/'gold.json')['cases']}
        for key, rule in [('redis','nonpositive'),('s3','s3-general-purpose')]:
            request={'query':qs[key]['question_ja'],'query_expansions':[en[key]],'max_tokens':1024,
                'documents':docs(OLD/f'{key}-documents.json')}
            before=invoke(request,'--query-rules','off')
            after=invoke(request,'--query-rules','builtin','--rule-ids',rule)
            baseline=score_units(before['documents'],gold[key]); selected=score_units(after['documents'],gold[key])
            assert set(baseline)<set(selected),(key,baseline,selected)
            fingerprint=invoke(request,'--rule-request-fingerprint')['request_sha256']
            for label, value in [('off',before),('subset',after)]:
                validate_provenance(value['documents'],request['documents'])
                assert support.count(json.dumps(value,ensure_ascii=False,separators=(',',':')))==value['tokens']<=1024
                output_path=HERE/'outputs'/f'{key}-{label}.json'
                if write: save(output_path,value)
                else: assert load(output_path)==value
            proof={'id':key,'request_sha256':fingerprint,'rule_ids':[rule],
                'engine_sha256':policy['engine_sha256'],'catalog_sha256':policy['catalog_sha256'],
                'input_files':{str(p.relative_to(REPO)):sha(p) for p in [OLD/'questions.json',OLD/'generated_english.json',OLD/'gold.json',OLD/f'{key}-documents.json']},
                'baseline_units':baseline,'selected_units':selected,
                'outputs':{f'outputs/{key}-{s}.json':sha(HERE/'outputs'/f'{key}-{s}.json') for s in ['off','subset']}}
            path=HERE/f'{key}-proof.json'
            if write: save(path,proof)
            else: assert load(path)==proof
            approvals.append({'id':f'{key}-1024-captured-v1','request_sha256':fingerprint,'rule_ids':[rule],
                'evidence':{'path':str(path.relative_to(REPO)),'sha256':sha(path),
                    'baseline_units':baseline,'selected_units':selected}})
            if not write:
                active=invoke(request);assert active==after
                # Every dimension of the approved condition is bound; no silent broadening.
                for change in ['budget','english','source','query']:
                    altered=json.loads(json.dumps(request))
                    if change=='budget': altered['max_tokens']=1025
                    elif change=='english': altered['query_expansions'][0]+=' '
                    elif change=='source': altered['documents'][0]['blocks'][-1]['text']+=' '
                    else: altered['query']+=' '
                    assert invoke(altered)==invoke(altered,'--query-rules','off'),(key,change)
            rows.append({'id':key,'budget':1024,'baseline_units':baseline,'subset_units':selected,
                'default_matches_verified_subset':not write,'mutated_conditions_match_off':not write})
        if write:
            save(REPO/'data/verified-query-rules.v1.json',{'schema_version':1,'version':'2026-10-22.1',
                'engine_sha256':policy['engine_sha256'],'catalog_sha256':policy['catalog_sha256'],'approvals':approvals})
            print(json.dumps({'proofs':rows,'next':'rebuild selector, rerun without --write-registry'},ensure_ascii=False))
            return
        # Historical artifacts remain unchanged; only frozen page/question/English/gold inputs reused.
        qs={c['id']:c for c in load(NEW/'questions.json')['questions']}
        en={c['id']:c['query_en'] for c in load(NEW/'improved_english.json')}
        gold={c['id']:c['units'] for c in load(NEW/'gold.json')['cases']}
        totals={1024:0,2048:0}
        for key,c in qs.items():
            for budget in [1024,2048]:
                request={'query':c['question_ja'],'query_expansions':[en[key]],'max_tokens':budget,
                    'documents':docs(NEW/f'{key}-documents.json')}
                standard=invoke(request);off=invoke(request,'--query-rules','off')
                assert standard==off,(key,budget)
                validate_provenance(standard['documents'],request['documents'])
                assert support.count(json.dumps(standard,ensure_ascii=False,separators=(',',':')))==standard['tokens']<=budget
                units=score_units(standard['documents'],gold[key]);totals[budget]+=len(units)
                save(HERE/'outputs'/f'{key}-{budget}-standard.json',standard)
                rows.append({'id':key,'budget':budget,'retained_units':units,'total_units':len(gold[key]),'default_matches_off':True})
        assert totals=={1024:13,2048:17},totals
        for base,names in [(OLD,['questions.json','generated_english.json','gold.json','redis-documents.json','s3-documents.json']),
                           (NEW,['questions.json','improved_english.json','gold.json']+[f'{key}-documents.json' for key in qs])]:
            for name in names:
                p=base/name;sources[str(p.relative_to(REPO))]=sha(p)
        save(HERE/'results.json',{'policy':policy,'selector_sha256':sha(SELECTOR),'sources':sources,
            'rows':rows,'standard_improved_english_totals':totals,'denominator':17,'ledger_audit':audit_ledger()})
        print(json.dumps({'totals':totals,'verified_cases':rows[:2]},ensure_ascii=False))
    finally: support.close()

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--write-registry',action='store_true');p.add_argument('--audit-only',action='store_true')
    args=p.parse_args()
    if args.audit_only:
        assert not args.write_registry
        value=audit_ledger();save(HERE/'ledger_audit.json',value);print(json.dumps(value))
    else: main(args.write_registry)
