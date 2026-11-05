// node prep.js -> _play.html (injeta timeline, grafo e crops)
const fs=require('fs');
fs.writeFileSync('_play.html',fs.readFileSync('index.html','utf8')
 .replace('__TL__',fs.readFileSync('timeline.json','utf8'))
 .replace('__GRAPH__',fs.readFileSync('graph.json','utf8'))
 .replace('__CROPS__',fs.readFileSync('crops.json','utf8')));
