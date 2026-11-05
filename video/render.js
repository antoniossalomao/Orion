const { chromium } = require('playwright'); const { spawn } = require('child_process');
(async()=>{
const b=await chromium.launch();const p=await b.newPage({viewport:{width:1920,height:1080}});
p.on('pageerror',e=>console.log('ERR',e.message));
const [html,out]=[process.argv[2]||'_play.html',process.argv[3]||'video.mp4'];await p.goto('file://'+process.cwd()+'/'+html);await p.waitForTimeout(800);
const FPS=30,total=await p.evaluate(()=>TOTAL),N=Math.ceil(total*FPS);
const ff=spawn('ffmpeg',['-y','-v','error','-f','image2pipe','-framerate',String(FPS),'-c:v','mjpeg','-i','-','-i','audio/mix.wav','-af','apad','-t',String(total),
 '-c:v','libx264','-preset','medium','-crf','17','-pix_fmt','yuv420p','-c:a','aac','-b:a','192k','-movflags','+faststart',out],{stdio:['pipe','inherit','inherit']});
for(let i=0;i<N;i++){await p.evaluate(t=>render(t),i/FPS);
 const buf=await p.screenshot({type:'jpeg',quality:94});
 if(!ff.stdin.write(buf))await new Promise(r=>ff.stdin.once('drain',r));
 if(i%150==0)console.log('frame',i,'/',N);}
ff.stdin.end();await new Promise(r=>ff.on('close',r));await b.close();console.log('DONE');})();
