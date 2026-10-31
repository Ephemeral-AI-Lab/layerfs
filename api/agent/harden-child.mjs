const { LayerFS } = await import("@layerfs/agent");
try {
  const q = await LayerFS.open("hard");
  const text = await q.fs.read("/README.md");
  console.log("OPENED");
} catch (e) { console.log("REFUSED " + String(e.message ?? e).slice(0, 200)); }
process.exit(0);
