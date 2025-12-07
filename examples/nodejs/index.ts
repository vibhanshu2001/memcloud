import { MemCloud } from 'memcloud';

const cloud = new MemCloud();

async function main() {
    await cloud.connect();

    const handle = await cloud.store("My Data");
    console.log("Stored ID:", handle.id);

    const data = await cloud.load(handle.id);
    console.log("Data:", data.toString());

    await cloud.set("app-config", JSON.stringify({ theme: "dark" }));
    const config = await cloud.get("app-config");
    console.log("Config:", JSON.parse(config.toString()));
    const peers = await cloud.peers();
    console.log("Peers:", peers);

    if (peers.length > 0) {
        const targetPeerString = peers[0]; // Pick first peer
        // Format is: "UUID (Name) @ Addr" - extract UUID
        const targetPeerId = targetPeerString.split(' ')[0];

        console.log(`Attempting to store data on peer: ${targetPeerString} (ID: ${targetPeerId})`);
        try {
            const remoteHandle = await cloud.store("Data for neighbor", targetPeerId);
            console.log("Remote Stored ID:", remoteHandle.id);

            // Verify retrieval
            console.log("Reading back remote data...");
            const remoteData = await cloud.load(remoteHandle.id);
            console.log("Remote Data:", remoteData.toString());
        } catch (e) {
            console.error("Remote store failed:", e);
        }
    } else {
        console.log("No peers connected to test remote storage.");
    }

    cloud.close();
}

main();