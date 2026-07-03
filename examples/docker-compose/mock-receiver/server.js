import http from "node:http";

const received = [];
const port = Number(process.env.PORT ?? 3000);

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "localhost"}`);

  if (req.method === "GET" && url.pathname === "/health") {
    return sendJson(res, 200, { status: "ok", received_batches: received.length });
  }

  if (req.method === "GET" && url.pathname === "/received") {
    return sendJson(res, 200, { batches: received });
  }

  if (req.method === "POST" && url.pathname === "/orders") {
    const body = await readJson(req);
    const batch = {
      received_at: new Date().toISOString(),
      authorization: req.headers.authorization ?? null,
      source: req.headers["x-source"] ?? null,
      body,
    };
    received.push(batch);
    console.log(JSON.stringify({ event: "orders_received", batch }));
    return sendJson(res, 202, { accepted: true, batches: received.length });
  }

  sendJson(res, 404, { error: "not found" });
});

server.listen(port, "0.0.0.0", () => {
  console.log(JSON.stringify({ event: "mock_receiver_started", port }));
});

async function readJson(req) {
  const chunks = [];
  for await (const chunk of req) {
    chunks.push(chunk);
  }
  const raw = Buffer.concat(chunks).toString("utf8");
  return raw ? JSON.parse(raw) : null;
}

function sendJson(res, status, body) {
  const json = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(json),
  });
  res.end(json);
}
