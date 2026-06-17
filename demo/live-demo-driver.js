const WebSocket = require("ws");

const port = Number(process.env.ASEPRITE_MCP_LIVE_PORT || 9876);
const server = new WebSocket.Server({ host: "127.0.0.1", port });

let nextId = 1;
let socket = null;
const pending = new Map();
let helloResolve = null;
let helloPromise = new Promise((resolve) => {
  helloResolve = resolve;
});

function command(type, target, payload) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    throw new Error("Aseprite plugin is not connected");
  }

  const id = `demo-${nextId++}`;
  const request = {
    protocol: "aseprite-live-edit",
    version: 1,
    id,
    type,
  };
  if (target) request.target = target;
  if (payload) request.payload = payload;

  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Timed out waiting for ${id}`));
    }, 5000);

    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify(request));
  });
}

function pushPixel(pixels, x, y, color) {
  if (x < 0 || y < 0 || x >= 32 || y >= 32) return;
  pixels.push({ x, y, color });
}

function pushShape(pixels, shape, ox, oy, color) {
  for (let y = 0; y < shape.length; y++) {
    for (let x = 0; x < shape[y].length; x++) {
      if (shape[y][x] === "1") {
        pushPixel(pixels, x + ox, y + oy, color);
      }
    }
  }
}

function pickupItemFramePixels(frameIndex) {
  const bob = [0, -1, -2, -1, 0, 1, 2, 1][frameIndex];
  const shape = [
    "01100110",
    "11111111",
    "11111111",
    "11111111",
    "01111110",
    "00111100",
    "00011000",
    "00000000",
  ];
  const pixels = [];
  const x = 12;
  const y = 8 + bob;

  pushShape(pixels, [
    "1111111111",
    "1111111111",
    "1111111111",
    "1111111111",
    "0111111110",
    "0011111100",
    "0001111000",
    "0000110000",
  ], x - 1, y - 1, "#6d1f36ff");
  pushShape(pixels, shape, x, y, "#ff3b5cff");
  pushShape(pixels, [
    "01000000",
    "10010000",
    "00000000",
  ], x + 1, y + 1, "#ff9bb0ff");
  pushShape(pixels, [
    "00000110",
    "00001111",
    "00001110",
    "00000100",
  ], x, y + 1, "#d9254aff");

  const shadowWidth = [5, 4, 3, 4, 5, 6, 7, 6][frameIndex];
  const shadowStart = 16 - Math.floor(shadowWidth / 2);
  for (let sx = 0; sx < shadowWidth; sx++) {
    pushPixel(pixels, shadowStart + sx, 24, "#2f314488");
  }

  return pixels;
}

function sparkleFramePixels(frameIndex) {
  const pixels = [];
  const sparkleFrames = [
    [{ x: 9, y: 6 }, { x: 24, y: 11 }],
    [{ x: 8, y: 7 }, { x: 24, y: 9 }],
    [{ x: 10, y: 5 }, { x: 23, y: 8 }],
    [{ x: 11, y: 6 }, { x: 25, y: 10 }],
    [{ x: 23, y: 7 }, { x: 8, y: 12 }],
    [{ x: 24, y: 8 }, { x: 9, y: 13 }],
    [{ x: 22, y: 6 }, { x: 10, y: 12 }],
    [{ x: 23, y: 5 }, { x: 8, y: 10 }],
  ];
  for (const sparkle of sparkleFrames[frameIndex]) {
    pushPixel(pixels, sparkle.x, sparkle.y, "#ffffffff");
    pushPixel(pixels, sparkle.x - 1, sparkle.y, "#ffd86bff");
    pushPixel(pixels, sparkle.x + 1, sparkle.y, "#ffd86bff");
    pushPixel(pixels, sparkle.x, sparkle.y - 1, "#ffd86bff");
    pushPixel(pixels, sparkle.x, sparkle.y + 1, "#ffd86bff");
  }

  return pixels;
}

server.on("connection", async (ws) => {
  socket = ws;
  console.log("Aseprite plugin connected");

  ws.on("message", (data) => {
    const text = data.toString();
    console.log("<<", text);
    let message;
    try {
      message = JSON.parse(text);
    } catch (error) {
      console.error("Invalid JSON from plugin", error);
      return;
    }

    if (message.type === "hello") {
      console.log("Plugin hello received");
      helloResolve(message);
      return;
    }

    const waiter = pending.get(message.id);
    if (!waiter) return;
    clearTimeout(waiter.timer);
    pending.delete(message.id);

    if (message.ok) waiter.resolve(message.result);
    else waiter.reject(new Error(JSON.stringify(message.error)));
  });

  ws.on("close", () => {
    console.log("Aseprite plugin disconnected");
  });

  try {
    await Promise.race([
      helloPromise,
      new Promise((resolve) => setTimeout(resolve, 1500)),
    ]);
    await new Promise((resolve) => setTimeout(resolve, 3000));

    console.log(">> ensure_layer");
    console.log(await command("ensure_layer", null, { name: "AI Draft" }));
    console.log(await command("ensure_layer", null, { name: "AI Sparkles" }));

    console.log(">> ensure_frames");
    console.log(await command("ensure_frames", null, { count: 8, duration: 0.11 }));

    for (let frame = 1; frame <= 8; frame++) {
      console.log(`>> clear item frame ${frame}`);
      console.log(await command("clear_cel", { layer: "AI Draft", frame }, null));
      console.log(`>> clear sparkle frame ${frame}`);
      console.log(await command("clear_cel", { layer: "AI Sparkles", frame }, null));

      console.log(`>> draw pickup item frame ${frame}`);
      console.log(await command(
        "draw_pixels",
        { layer: "AI Draft", frame },
        { pixels: pickupItemFramePixels(frame - 1) },
      ));

      console.log(`>> draw sparkles frame ${frame}`);
      console.log(await command(
        "draw_pixels",
        { layer: "AI Sparkles", frame },
        { pixels: sparkleFramePixels(frame - 1) },
      ));
    }

    console.log("Pickup animation completed. Press Play in Aseprite to preview the bobbing collectible.");
  } catch (error) {
    console.error("Demo failed:", error);
  }
});

server.on("listening", () => {
  console.log(`Live demo WebSocket listening on ws://127.0.0.1:${port}`);
  console.log("In Aseprite, use Help > MCP Server > Connect to MCP Server.");
});
