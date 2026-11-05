// node prep.js -> _play.html (injeta o grafo gerado por ../graph_build.py)
const fs=require('fs');fs.writeFileSync('_play.html',fs.readFileSync('index.html','utf8').replace('__GRAPH__',fs.readFileSync('../graph.json','utf8')));
