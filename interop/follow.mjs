// Takes the browser on <port> into a room on the deployment at <base> as an
// ordinary participant, with every peer connection it makes kept in
// `window.__pcs` for sent.js and channels.js to read, and leaves it there.
//   node follow.mjs <port> <base> <room> [invite]
const [port, BASE, ROOM, INVITE] = process.argv.slice(2);
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const ws = await new Promise((ok, bad) => {
	const s = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
	s.onopen = () => ok(s);
	s.onerror = bad;
});
let id = 0;
const send = (method, params = {}) => new Promise((ok, bad) => {
	const mine = ++id;
	const on = (e) => {
		const x = JSON.parse(e.data);
		if (x.id !== mine) return;
		ws.removeEventListener("message", on);
		x.error ? bad(new Error(JSON.stringify(x.error))) : ok(x.result);
	};
	ws.addEventListener("message", on);
	ws.send(JSON.stringify({ id: mine, method, params }));
});
const run = async (expression) => {
	const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
	return r.exceptionDetails ? "EXC:" + JSON.stringify(r.exceptionDetails).slice(0, 300) : r.result.value;
};

await send("Page.enable");
await send("Runtime.enable");
await send("Page.navigate", { url: "about:blank" });
await send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => {
	const Original = window.RTCPeerConnection; window.__pcs = [];
	function Hooked(...a) { const pc = new Original(...a); window.__pcs.push(pc); return pc; }
	Hooked.prototype = Original.prototype; Object.setPrototypeOf(Hooked, Original);
	window.RTCPeerConnection = Hooked;
})()` });
// Storage is cleared at the deployment's own origin; at about:blank it would
// clear about:blank's.
await send("Page.navigate", { url: `${BASE}/robots.txt` });
await wait(800);
await run("localStorage.clear(); sessionStorage.clear(); 'cleared'");
await send("Page.navigate", { url: "about:blank" });
await wait(300);
await send("Page.navigate", { url: `${BASE}/${INVITE ? `?invite=${INVITE}` : ""}#/${ROOM}` });
await wait(3500);

const name = await run(`(() => {
	const input = document.querySelector('input[aria-label="Your name"]');
	if (!input) return "no name field";
	const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
	set.call(input, "Web Peer");
	input.dispatchEvent(new Event("input", { bubbles: true }));
	return "ok";
})()`);
console.log("name:", name);
await wait(300);
console.log("join:", await run(`(() => {
	const b = [...document.querySelectorAll("button")].find((x) => /join/i.test(x.textContent));
	if (!b) return "no join button: " + [...document.querySelectorAll("button")].map((x) => x.textContent.trim()).join("|");
	if (b.disabled) return "disabled";
	b.click();
	return "clicked";
})()`));
await wait(6000);
console.log("peer connections:", await run("(window.__pcs || []).map((pc) => pc.connectionState).join(',')"));
ws.close();
