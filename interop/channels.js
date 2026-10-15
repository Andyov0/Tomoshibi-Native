(async () => {
  const out = [];
  for (const pc of window.__pcs) for (const r of pc.getReceivers()) {
    if (!r.track || r.track.kind !== "audio") continue;
    const ctx = new AudioContext({ sampleRate: 48000 });
    await ctx.resume().catch(() => {});
    const el = new Audio(); el.muted = true; el.srcObject = new MediaStream([r.track]); el.play().catch(() => {});
    const src = ctx.createMediaStreamSource(new MediaStream([r.track]));
    const split = ctx.createChannelSplitter(2);
    src.connect(split);
    const peaks = [];
    const analysers = [0, 1].map((c) => { const a = ctx.createAnalyser(); a.fftSize = 8192; split.connect(a, c); return a; });
    await new Promise((ok) => setTimeout(ok, 1500));
    for (const a of analysers) {
      const bins = new Float32Array(a.frequencyBinCount); a.getFloatFrequencyData(bins);
      let best = 0; for (let i = 1; i < bins.length; i++) if (bins[i] > bins[best]) best = i;
      peaks.push(Math.round(best * ctx.sampleRate / a.fftSize) + "Hz@" + Math.round(bins[best]) + "dB");
    }
    out.push({ mid: pc.getTransceivers().find((t) => t.receiver === r)?.mid, state: ctx.state, left: peaks[0], right: peaks[1] });
    ctx.close();
  }
  return JSON.stringify(out);
})()
