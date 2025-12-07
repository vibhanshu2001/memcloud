import { MemSocket } from './socket';

export interface Handle {
    id: string; // Now safe as string
}

export class MemCloud {
    private socket: MemSocket;

    constructor(pathOrPort: string | number = '/tmp/memcloud.sock') {
        this.socket = new MemSocket(pathOrPort);
    }

    async connect() {
        await this.socket.connect();
        console.log("Connected to MemCloud Daemon");
    }

    async store(data: string | Buffer, target?: string): Promise<Handle> {
        const payload = Buffer.isBuffer(data) ? data : Buffer.from(data);

        let cmd: any;
        if (target) {
            console.log(`Storing ${payload.length} bytes on peer '${target}'...`);
            cmd = { cmd: 'StoreRemote', data: payload, target };
        } else {
            console.log(`Storing ${payload.length} bytes...`);
            cmd = { cmd: 'Store', data: payload };
        }

        const resp = await this.socket.request(cmd);

        if (resp.res === 'Stored') {
            console.log(`Stored Block ID: ${resp.id}`);
            return { id: resp.id };
        } else if (resp.res === 'Error') {
            throw new Error(resp.msg);
        }
        throw new Error("Unknown response: " + JSON.stringify(resp));
    }

    async load(idOrHandle: string | Handle): Promise<Buffer> {
        const id = typeof idOrHandle === 'string' ? idOrHandle : idOrHandle.id;
        console.log(`Loading Block ID: ${id}...`);
        const resp = await this.socket.request({ cmd: 'Load', id });

        if (resp.res === 'Loaded') {
            // data is Buffer (msgpackr)
            const buf = resp.data;
            console.log(`Loaded ${buf.length} bytes.`);
            return buf;
        } else if (resp.res === 'Error') {
            throw new Error(resp.msg);
        }
        throw new Error("Unknown response: " + JSON.stringify(resp));
    }

    async set(key: string, data: string | Buffer): Promise<Handle> {
        const payload = Buffer.isBuffer(data) ? data : Buffer.from(data);

        console.log(`Setting '${key}'...`);
        const resp = await this.socket.request({ cmd: 'Set', key, data: payload });

        if (resp.res === 'Stored') {
            console.log(`Set '${key}' -> ID: ${resp.id}`);
            return { id: resp.id };
        } else if (resp.res === 'Error') {
            throw new Error(resp.msg);
        }
        throw new Error("Unknown response: " + JSON.stringify(resp));
    }

    async get(key: string): Promise<Buffer> {
        console.log(`Getting '${key}'...`);
        const resp = await this.socket.request({ cmd: 'Get', key });

        if (resp.res === 'Loaded') {
            const buf = resp.data;
            console.log(`Got '${key}': ${buf.length} bytes.`);
            return buf;
        } else if (resp.res === 'Error') {
            throw new Error(resp.msg);
        }
        throw new Error("Unknown response: " + JSON.stringify(resp));
    }

    async peers(): Promise<string[]> {
        console.log("Listing peers...");
        const resp = await this.socket.request({ cmd: 'ListPeers' });

        if (resp.res === 'List') {
            return resp.items;
        } else if (resp.res === 'Error') {
            throw new Error(resp.msg);
        }
        throw new Error("Unknown response: " + JSON.stringify(resp));
    }

    async free(id: string): Promise<void> {
        // id is string
        const resp = await this.socket.request({ cmd: 'Free', id });
        if (resp.res === 'Success') {
            return;
        } else if (resp.res === 'Error') {
            throw new Error(resp.msg);
        }
        throw new Error("Unknown response: " + JSON.stringify(resp));
    }

    disconnect() {
        this.socket.end();
    }

    /** Alias for disconnect() */
    close() {
        this.disconnect();
    }
}
