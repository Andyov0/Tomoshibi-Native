// Evaluated in the page (peek.mjs): the tracks the media server is sending
// this browser now, read off the remote descriptions of its live peer
// connections. A track the sender unpublished is renegotiated to inactive and
// leaves this list; one it only stopped feeding stays in it, which is how a
// frozen camera or a stage stuck on the last shared screen is told from a
// track that is really gone. Needs the RTCPeerConnection hook web-peer.mjs and
// follow.mjs install (`window.__pcs`).
(() => {
	const sent = [];
	for (const pc of window.__pcs || []) {
		if (pc.connectionState === "closed" || !pc.remoteDescription) continue;
		for (const section of pc.remoteDescription.sdp.split(/\r?\nm=/).slice(1)) {
			const [kind, port] = section.split(" ");
			const direction = (section.match(/a=(sendonly|sendrecv|recvonly|inactive)/) || [])[1];
			const msid = section.match(/a=msid:(\S+) (\S+)/);
			if (port === "0" || !msid || (direction !== "sendonly" && direction !== "sendrecv")) continue;
			sent.push({ kind, mid: (section.match(/a=mid:(\S+)/) || [])[1], stream: msid[1], track: msid[2] });
		}
	}
	return JSON.stringify(sent);
})()
