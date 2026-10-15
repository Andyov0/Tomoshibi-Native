const [port, expr] = process.argv.slice(2);
const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const ws = await new Promise((ok, bad) => { const s = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl); s.onopen = () => ok(s); s.onerror = bad; });
ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression: expr, awaitPromise: true, returnByValue: true } }));
ws.onmessage = (e) => { const x = JSON.parse(e.data); if (x.id === 1) { console.log(x.result.exceptionDetails ? JSON.stringify(x.result.exceptionDetails).slice(0, 300) : x.result.result.value); process.exit(0); } };
