// Exporta o logo em PNG transparente (3800x1000) e o símbolo (3 pontos) em 1024x1024.
const {chromium}=require('playwright');
(async()=>{const b=await chromium.launch();const p=await b.newPage({viewport:{width:3780,height:1000}});
await p.goto('file://'+__dirname+'/logo.html');await p.evaluate(()=>document.fonts.ready);await p.waitForTimeout(300);
await p.locator('#logo').screenshot({path:__dirname+'/orion-logo.png',omitBackground:true});
await p.setContent(`<svg id="m" xmlns="http://www.w3.org/2000/svg" viewBox="-60 -50 260 260" width="1024" height="1024"><defs><radialGradient id="d" cx="38%" cy="32%" r="75%"><stop offset="0" stop-color="#fff"/><stop offset=".55" stop-color="#eef1fb"/><stop offset="1" stop-color="#c9d0e6"/></radialGradient></defs><circle cx="0" cy="50" r="20" fill="url(#d)"/><circle cx="70" cy="80" r="20" fill="url(#d)"/><circle cx="140" cy="110" r="20" fill="url(#d)"/></svg>`);
await p.locator('#m').screenshot({path:__dirname+'/orion-simbolo.png',omitBackground:true});await b.close()})();
