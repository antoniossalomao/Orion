// Frames de conferência: node preview.js _play.html 5 20 40  -> build/p_<t>.jpg ; depois python3 contact.py 5 20 40
const { chromium } = require('playwright');const fs=require('fs');fs.mkdirSync('build',{recursive:true});
(async()=>{const b=await chromium.launch();const p=await b.newPage({viewport:{width:1920,height:1080}});p.on('pageerror',e=>console.log('ERR',e.message));
await p.goto('file://'+process.cwd()+'/'+process.argv[2]);await p.waitForTimeout(500);
for(const t of process.argv.slice(3).map(Number)){await p.evaluate(t=>render(t),t);await p.screenshot({path:`build/p_${t}.jpg`,type:'jpeg',quality:80})}await b.close()})();
