"""Native config/protocol equivalence checks. No model turn or embedding service."""
import hashlib,json,os,pathlib,shutil,subprocess,tempfile,tomllib
ROOT=pathlib.Path(__file__).resolve().parents[2];OUT=pathlib.Path(__file__).resolve().parent
ENV=dict(os.environ);ENV.pop("JRG_EMBEDDING_SOCKET",None)
fixture=OUT/"guard.rs";fixture.write_text('fn receive(status: &str, candidates: &[u8], version: u32) {\n    if status != "completed"\n        || !candidates.is_empty()\n        || version == 0\n    { return; }\n}\n')
args={"operations":[{"read":{"path":str(fixture.relative_to(ROOT)),"start":1,"end":6}}],"requirements":[{"id":"R4","description":"状態と件数と版","patterns":["status !="]}],"max_tokens":1200}
requests=[{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"tools/list"}]
for i in (3,4):requests.append({"jsonrpc":"2.0","id":i,"method":"tools/call","params":{"name":"retrieve","arguments":args}})
requests.append({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"retrieve","arguments":{"operations":[{"read":{"path":"/etc/hosts","start":1,"end":1}}]}}})
wire="".join(json.dumps(x)+"\n" for x in requests)
def exchange(command):
 p=subprocess.run(command,cwd=ROOT,env=ENV,input=wire,text=True,capture_output=True,timeout=20,check=True)
 return [json.loads(line) for line in p.stdout.splitlines()]
config=tomllib.loads((ROOT/".codex/config.toml").read_text())["mcp_servers"]["jrg"]
commands={"Codex":[config["command"],*config["args"]]}
for name,file in [("Claude",".mcp.json"),("Cursor",".cursor/mcp.json")]:
 c=json.loads((ROOT/file).read_text())["mcpServers"]["jrg"];commands[name]=[c["command"],*c["args"]]
c=json.loads((ROOT/"opencode.json").read_text())["mcp"]["jrg"];assert c["type"]=="local" and c["enabled"];commands["OpenCode"]=c["command"]
assert len({tuple(v) for v in commands.values()})==1
installed=pathlib.Path.home()/".local/bin/jrg";binary_hash=hashlib.sha256(installed.read_bytes()).hexdigest();assert binary_hash==hashlib.sha256((ROOT/"target/release/jrg").read_bytes()).hexdigest()
expected=(ROOT/"src/mcp_instructions.txt").read_text();assert len(expected)<=4096
assert "Prefer jrg" in expected[:512] and all(s in expected[:512] for s in ["exhaustive","machine","unavailable","configured root"])
with tempfile.TemporaryDirectory() as state:
 baseline=exchange([str(ROOT/"experiments/2026-10-17-integration-cache/jrg-0.15.1"),"--smart","--mcp","--root",str(ROOT),"--session",str(pathlib.Path(state)/"old.json")])
results={}
for name,command in commands.items():
 replies=exchange(command);assert replies[0]["result"]["serverInfo"]["version"]=="0.15.2"
 assert replies[0]["result"]["instructions"]==expected
 assert replies[1]["result"]["tools"][0]["name"]=="retrieve"
 assert [x["result"] for x in replies[2:]]==[x["result"] for x in baseline[2:]],name
 texts=[x["result"]["content"][0]["text"] for x in replies[2:4]]
 assert "seen=0" in texts[0] and "seen=6" in texts[1]
 for text in texts:
  assert "# condition R4" in text and all(x in text for x in ['status != "completed"','|| !candidates.is_empty()','|| version == 0'])
 assert replies[-1]["result"]["isError"] and "escapes configured MCP root" in replies[-1]["result"]["content"][0]["text"]
 results[name]={"command":command,"initialize":replies[0],"retrieve_equivalent_to_0151":True,"replies":replies[2:]}
# Confirm documented rule entry points and imports; this is configuration validation, not model adherence.
policy=(ROOT/"AGENTS.md").read_text();claude=(ROOT/"CLAUDE.md").read_text();cursor=(ROOT/".cursor/rules/jrg.mdc").read_text()
assert "@AGENTS.md" in claude and "alwaysApply: true" in cursor.split("---")[1]
rg=pathlib.Path(shutil.which("rg")).resolve();before=json.loads((OUT/"rg-before.json").read_text())
assert str(rg)==before["path"] and hashlib.sha256(rg.read_bytes()).hexdigest()==before["sha256"]
relative=str(fixture.relative_to(ROOT))
matched=subprocess.run([str(rg),"--json","-F","completed",relative],cwd=ROOT,env=ENV,text=True,capture_output=True,check=True)
records=[json.loads(line) for line in matched.stdout.splitlines()];assert sum(x["type"]=="match" for x in records)==1
assert subprocess.run([str(rg),"--quiet","-F","JRG_NO_SUCH_MARKER_7621",relative],cwd=ROOT,env=ENV).returncode==1
summary={"installed_sha256":binary_hash,"version":"0.15.2","model_turns":0,"embedding_calls":0,"instructions_length":len(expected),"four_configs_protocol_verified":list(results),"equivalent_to_0151":True,"rules":{"Codex":"AGENTS.md","Claude":"CLAUDE.md imports AGENTS.md","Cursor":".cursor/rules/jrg.mdc alwaysApply=true","OpenCode":"AGENTS.md"},"rg":{"unchanged":True,"json_matches":1,"absence_exit":1},"model_selection_rate":"unmeasured"}
(OUT/"verification.json").write_text(json.dumps({"summary":summary,"clients":results},ensure_ascii=False,indent=2)+"\n")
print(json.dumps(summary,ensure_ascii=False))
