// Exporta a lista de efeitos sonoros (tempos tirados da própria cena) para sfx.json.
const {chromium}=require('playwright');const fs=require('fs');
(async()=>{const b=await chromium.launch();const p=await b.newPage();await p.goto('file://'+process.cwd()+'/_play.html');
fs.writeFileSync('sfx.json',JSON.stringify(await p.evaluate(()=>sfxList())));await b.close();console.log('sfx.json')})();
