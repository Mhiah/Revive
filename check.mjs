const rpc="https://test.rpc.fastnear.com";
const q=async(params)=>{const r=await fetch(rpc,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({jsonrpc:"2.0",id:1,method:"query",params})});return r.json();};
const tags=["6tx39","ztw7t","bhtat","cia7y","vft8z","5svgh"];
const roles=["dev","alice","bob","carol","whale","stewards","token","meme"];
let tot=0;
for(const t of tags) for(const r of roles){
  const id=`${r}-${t}.revive.testnet`;
  const j=await q({request_type:"view_account",finality:"final",account_id:id});
  if(j.result){const n=Number(BigInt(j.result.amount)/10n**21n)/1000; const st=j.result.storage_usage; const free=n-st*1e-5; tot+=Math.max(0,free-0.05); console.log(id.padEnd(40),n.toFixed(3),"NEAR, storage",st,"B, spare",free.toFixed(3));}
}
const root=await q({request_type:"view_account",finality:"final",account_id:"revive.testnet"});
console.log("revive.testnet",Number(BigInt(root.result.amount)/10n**21n)/1000);
console.log("TOTAL spare (keeping 0.05 each):",tot.toFixed(2));
