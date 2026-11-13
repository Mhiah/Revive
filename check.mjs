const rpc="https://test.rpc.fastnear.com";
const q=async(params)=>{const r=await fetch(rpc,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({jsonrpc:"2.0",id:1,method:"query",params:{finality:"final",...params}})});return r.json();};
const view=async(a,m,args={})=>{const j=await q({request_type:"call_function",account_id:a,method_name:m,args_base64:Buffer.from(JSON.stringify(args)).toString("base64")});if(j.error||j.result?.error)return "ERR:"+(j.result?.error||JSON.stringify(j.error)).slice(0,80);return JSON.parse(Buffer.from(j.result.result).toString()||"null");};
const keys=async(a)=>{const j=await q({request_type:"view_access_key_list",account_id:a});return j.result?j.result.keys.length:"ERR";};
const acct=async(a)=>{const j=await q({request_type:"view_account",account_id:a});return j.result?{code:j.result.code_hash,near:Number(BigInt(j.result.amount)/10n**21n)/1000}:"ERR";};
for (const f of ["revive-5svgh.revive.testnet","revive-6tx39.revive.testnet","revive-ztw7t.revive.testnet","revive-bhtat.revive.testnet"]) {
  console.log("FACTORY",f,JSON.stringify({keys:await keys(f),acct:await acct(f),owner:await view(f,"get_owner"),code:await view(f,"get_code_hash"),guard:await view(f,"get_guard_code_hash"),suffixes:await view(f,"get_dao_suffixes"),count:await view(f,"get_coin_count")}));
  const coins=await view(f,"get_coins",{from_index:0,limit:100});
  if(Array.isArray(coins)) for(const c of coins){const a=await acct(c.account_id); console.log("  COIN",c.account_id,c.kind,JSON.stringify({keys:await keys(c.account_id),code:a.code,matches:a.code===(c.kind==="guard"?await view(f,"get_guard_code_hash"):await view(f,"get_code_hash"))}));}
}
console.log("OWNER revive.testnet keys:",await keys("revive.testnet"));
