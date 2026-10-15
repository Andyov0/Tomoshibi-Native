// The browser half of the native-core probe: follows the probe into its room by
// invitation and reports what a web participant actually receives from it.
//   node web-peer.mjs <port> <room> <invite> <seconds>
const [port, ROOM, INVITE, WORD = ""] = process.argv.slice(2);
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

function cdp(port) {
	let ws, id = 0;
	const send = (m, params = {}) => new Promise((ok, bad) => {
		const mine = ++id;
		const on = (e) => { const x = JSON.parse(e.data); if (x.id !== mine) return; ws.removeEventListener("message", on); x.error ? bad(new Error(JSON.stringify(x.error))) : ok(x.result); };
		ws.addEventListener("message", on); ws.send(JSON.stringify({ id: mine, method: m, params }));
	});
	return {
		async open() {
			const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
			const target = list.find((t) => t.type === "page");
			ws = await new Promise((ok, bad) => { const s = new WebSocket(target.webSocketDebuggerUrl); s.onopen = () => ok(s); s.onerror = bad; });
			await send("Page.enable"); await send("Runtime.enable"); await send("DOM.enable");
		},
		go: (url) => send("Page.navigate", { url }),
		send,
		async run(e) { const r = await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true }); return r.exceptionDetails ? "EXC:" + JSON.stringify(r.exceptionDetails).slice(0, 300) : r.result.value; },
		close: () => ws.close(),
	};
}
async function type(page, selector, text) {
	const doc = await page.send("DOM.getDocument", {});
	const f = await page.send("DOM.querySelector", { nodeId: doc.root.nodeId, selector });
	if (!f.nodeId) return "not found";
	await page.send("DOM.focus", { nodeId: f.nodeId });
	for (const ch of text) { await page.send("Input.dispatchKeyEvent", { type: "keyDown", text: ch }); await page.send("Input.dispatchKeyEvent", { type: "keyUp" }); }
	return "ok";
}

const page = cdp(port);
await page.open();
await page.go("about:blank");
await page.send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => {
	const Original = window.RTCPeerConnection; window.__pcs = [];
	function Hooked(...a) { const pc = new Original(...a); window.__pcs.push(pc); return pc; }
	Hooked.prototype = Original.prototype; Object.setPrototypeOf(Hooked, Original);
	window.RTCPeerConnection = Hooked;
})()` });
await page.go("http://127.0.0.1:8080/robots.txt"); await wait(800);
await page.run("localStorage.clear(); sessionStorage.clear(); 'cleared'");
await page.go("about:blank"); await wait(300);
await page.go(`http://127.0.0.1:8080/?invite=${INVITE}#/${ROOM}`);
await wait(3500);
console.log("name:", await type(page, 'input[aria-label="Your name"]', "Web Peer"));
if (WORD) {
	await page.run(`(() => { const b = [...document.querySelectorAll("button")].find((x) => /seal this call/i.test(x.textContent || "")); if (b) b.click(); })()`);
	await wait(500);
	console.log("word:", await type(page, 'input[aria-label="Sealing word"]', WORD));
}
await wait(300);
console.log("join:", await page.run(`(() => { const b = [...document.querySelectorAll("button")].find(x => /join/i.test(x.textContent)); if (!b) return "no join button: " + [...document.querySelectorAll("button")].map(x=>x.textContent.trim()).join("|"); if (b.disabled) return "disabled"; b.click(); return "clicked"; })()`));
await wait(4000);
console.log("camera:", await page.run(`(() => { const b = [...document.querySelectorAll("button")].find(x => x.getAttribute("aria-label") === "Turn camera on"); if (!b) return "none"; b.click(); return "on"; })()`));
const measure = `(async () => {
	const out = { audio: [], video: [] };
	for (const pc of window.__pcs) {
		const stats = await pc.getStats();
		stats.forEach((s) => {
			if (s.type !== "inbound-rtp") return;
			if (s.kind === "audio") out.audio.push({ packets: s.packetsReceived, energy: Number((s.totalAudioEnergy || 0).toFixed(3)) });
			if (s.kind === "video") out.video.push({ frames: s.framesDecoded, size: s.frameWidth + "x" + s.frameHeight, dropped: s.framesDropped });
		});
	}
	return JSON.stringify(out);
})()`;
await wait(6000);
console.log("at 10s:", await page.run(measure));
await wait(6000);
console.log("at 16s:", await page.run(measure));
page.close();
process.exit(0);
