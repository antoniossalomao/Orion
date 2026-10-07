/* ==========================================================================
   ORION — voice-worklet.js | captura do microfone para a voz ao vivo (AudioWorklet)
   Roda na thread de áudio: junta os blocos de 128 amostras em pedaços de 2048 (~128 ms a 16 kHz)
   e manda o Float32 para a página, que converte para PCM e envia. Substitui o ScriptProcessor.
   ========================================================================== */
class CapturaOrion extends AudioWorkletProcessor {
    constructor() {
        super();
        this.buf = new Float32Array(2048);
        this.n = 0;
    }
    process(entradas) {
        const canal = entradas[0]?.[0];
        if (!canal) return true;
        for (let i = 0; i < canal.length; i++) {
            this.buf[this.n++] = canal[i];
            if (this.n === this.buf.length) {
                this.port.postMessage(this.buf.slice());
                this.n = 0;
            }
        }
        return true;
    }
}
registerProcessor('captura-orion', CapturaOrion);
