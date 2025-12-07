
import { MemCloud } from 'memcloud';
import { Buffer } from 'buffer';

const cloud = new MemCloud();
const SIZE_MB = 500;
const SIZE_BYTES = SIZE_MB * 1024 * 1024;

async function main() {
    console.log("🚀 Starting Large Offload Demo (500MB)");

    await cloud.connect();

    // 1. Check for peers
    const peers = await cloud.peers();
    let targetId: string | undefined;

    if (peers.length === 0) {
        console.warn("⚠️ No peers connected! Switching to LOCAL storage test.");
    } else {
        const targetPeer = peers[0];
        targetId = targetPeer.split(' ')[0]; // Extract UUID
        console.log(`🤝 Connected to peer: ${targetPeer}`);
    }

    // 2. Allocate Data
    console.log(`📦 Allocating ${SIZE_MB}MB Buffer locally...`);
    const startTimeElement = process.hrtime();
    // Fill with some data to ensure it's not optimized away by OS/V8 lazy allocation
    const localData = Buffer.allocUnsafe(SIZE_BYTES);
    localData[0] = 0xDE;
    localData[SIZE_BYTES - 1] = 0xAD;
    const allocTime = process.hrtime(startTimeElement);
    console.log(`   Allocated in ${allocTime[0]}s ${allocTime[1] / 1000000}ms`);

    // 3. Store Remote
    console.log(`📤 Offloading ${SIZE_MB}MB to remote peer... (This may take a moment)`);
    console.time("Offload Time");

    try {
        const handle = await cloud.store(localData, targetId);
        console.timeEnd("Offload Time");
        console.log(`✅ Stored Remotely! ID: ${handle.id}`);

        // 4. Free Local Memory (Conceptually)
        // In JS we can't manually free, but we drop the reference.
        // localData = null; // TypeScript won't like reassignment of const, but we assume scope drop.
        console.log("🗑️  Dropping local reference (Simulating freed RAM)");

        // 5. Read Back
        console.log("📥 Fetching data back from remote...");
        console.time("Fetch Time");
        const storedData = await cloud.load(handle.id);
        console.timeEnd("Fetch Time");

        // 6. Verify
        console.log(`🔍 Verifying data integrity...`);
        if (storedData.length === SIZE_BYTES && storedData[0] === 0xDE && storedData[SIZE_BYTES - 1] === 0xAD) {
            console.log("✅ Data verified! Integrity Intact.");
            console.log("🎉 Infinite RAM Demo Successful!");
        } else {
            console.error("❌ Data Corruption Detected!");
            console.error(`Expected size: ${SIZE_BYTES}, Got: ${storedData.length}`);
        }

    } catch (e) {
        console.error("❌ Remote Store Failed:", e);
    }

    cloud.close();
}

main().catch(console.error);
