import React, { useState, useEffect } from 'react';
import { Terminal, Cpu, Network, Shield, Copy, Check, Menu, X, ArrowRight, Cloud, Server, Github, Download } from 'lucide-react';
import { Button } from "@/components/ui/button";
import { Link } from 'react-router-dom';
import { Badge } from "@/components/Badge";

const CLI_VERSION = "1.3.0";

const CliDocs = () => {
    const [copied, setCopied] = useState<string | null>(null);
    const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
    const [activeSection, setActiveSection] = useState("introduction");

    const copyToClipboard = (text: string) => {
        navigator.clipboard.writeText(text);
        setCopied(text);
        setTimeout(() => setCopied(null), 2000);
    };

    const sections = [
        {
            id: "introduction",
            title: "Introduction",
            icon: Terminal,
            subItems: []
        },
        {
            id: "installation",
            title: "Installation",
            icon: Download,
            subItems: [
                { id: "install-script", title: "Direct Install (Unix)" },
                { id: "install-windows", title: "Windows Install" },
                { id: "install-npm", title: "JS SDK" }
            ]
        },
        {
            id: "peers",
            title: "Peer Management",
            icon: Network,
            subItems: [
                { id: "peer-connect", title: "Connect" },
                { id: "peer-list", title: "List Peers" },
                { id: "peer-update", title: "Update Quota" },
                { id: "peer-disconnect", title: "Disconnect" }
            ]
        },
        {
            id: "security",
            title: "Security & Trust",
            icon: Shield,
            subItems: [
                { id: "sec-list", title: "List Trusted" },
                { id: "sec-consent", title: "Consent Prompt" },
                { id: "sec-remove", title: "Remove Trust" }
            ]
        },
        {
            id: "storage",
            title: "Storage Operations",
            icon: Server,
            subItems: [
                { id: "store-text", title: "Store Data" },
                { id: "store-kv-set", title: "Key-Value Set" },
                { id: "store-kv-get", title: "Key-Value Get" }
            ]
        },
        {
            id: "system",
            title: "System Commands",
            icon: Cpu,
            subItems: [
                { id: "sys-start", title: "Start Daemon" },
                { id: "sys-stream", title: "Streaming" }
            ]
        },
        {
            id: "contributing",
            title: "Contributing",
            icon: Github,
            subItems: [
                { id: "contrib-source", title: "Build from Source" },
                { id: "contrib-daemon", title: "Running the Daemon" },
                { id: "contrib-cli", title: "Running the CLI" }
            ]
        }
    ];

    useEffect(() => {
        const handleScroll = () => {
            for (const section of sections) {
                const element = document.getElementById(section.id);
                if (element) {
                    const rect = element.getBoundingClientRect();
                    // If the section is still substantially visible (bottom > 120px from top)
                    // and since we iterate top-down, the first one matching this is the top-most visible one.
                    if (rect.bottom > 120) {
                        setActiveSection(section.id);
                        break;
                    }
                }
            }
        };

        window.addEventListener('scroll', handleScroll);
        return () => window.removeEventListener('scroll', handleScroll);
    }, []);

    const scrollToSection = (id: string) => {
        const element = document.getElementById(id);
        if (element) {
            element.scrollIntoView({ behavior: 'smooth' });
            setActiveSection(id);
            setMobileMenuOpen(false);
        }
    };

    const CommandBlock = ({ command, description }: { command: string, description: string }) => (
        <div className="bg-secondary/20 rounded-lg p-6 border border-border/50 my-6 group hover:border-primary/30 transition-colors">
            <div className="flex justify-between items-start mb-3 bg-black/40 rounded-md p-3 font-mono text-sm relative">
                <div className="text-code-green whitespace-pre-wrap break-all pr-8">
                    <span className="text-slate-500 select-none">$ </span>
                    {command}
                </div>
                <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => copyToClipboard(command)}
                    className="absolute right-2 top-2 h-6 w-6 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                >
                    {copied === command ? <Check className="h-3 w-3 text-green-500" /> : <Copy className="h-3 w-3" />}
                </Button>
            </div>
            <p className="text-muted-foreground text-sm leading-relaxed">{description}</p>
        </div>
    );

    return (
        <div className="min-h-screen bg-background text-foreground flex flex-col font-sans">
            {/* Header (Matching Homepage) */}
            <header className="sticky top-0 z-50 border-b border-border/50 bg-background/80 backdrop-blur-md">
                <div className="container mx-auto px-4 h-16 flex items-center justify-between">
                    <Link to="/" className="flex items-center gap-2 hover:opacity-80 transition-opacity">
                        <Cloud className="w-6 h-6 text-primary animate-pulse-glow" />
                        <span className="font-semibold text-lg">MemCloud</span>
                        <Badge variant="secondary" className="hidden sm:inline-flex text-xs h-5 ml-2">Docs</Badge>
                    </Link>

                    <div className="flex items-center gap-4">
                        <Link to="/" className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors hidden md:block">
                            Home
                        </Link>
                        <a href="https://github.com/vibhanshu2001/memcloud" target="_blank" rel="noopener noreferrer" className="p-2 text-muted-foreground hover:text-foreground transition-colors">
                            <Github className="w-5 h-5" />
                        </a>
                        <Button variant="ghost" className="md:hidden" onClick={() => setMobileMenuOpen(!mobileMenuOpen)}>
                            {mobileMenuOpen ? <X className="w-5 h-5" /> : <Menu className="w-5 h-5" />}
                        </Button>
                    </div>
                </div>
            </header>

            <div className="flex-1 container mx-auto flex">
                {/* Sidebar Navigation */}
                <aside className={`
            fixed md:sticky top-16 left-0 h-[calc(100vh-4rem)] w-64 bg-background border-r border-border/50
            overflow-y-auto transform transition-transform z-40 p-4 md:translate-x-0
            ${mobileMenuOpen ? 'translate-x-0' : '-translate-x-full md:block'}
        `}>
                    <div className="space-y-6">
                        <div>
                            <h4 className="px-4 py-2 text-sm font-semibold text-foreground/70 uppercase tracking-wider">Contents</h4>
                            <div className="space-y-1">
                                {sections.map(section => (
                                    <div key={section.id}>
                                        <button
                                            onClick={() => scrollToSection(section.id)}
                                            className={`
                                                w-full flex items-center gap-3 px-4 py-2 rounded-lg text-sm font-medium transition-colors
                                                ${activeSection === section.id
                                                    ? 'bg-primary/10 text-primary'
                                                    : 'text-muted-foreground hover:bg-secondary/50 hover:text-foreground'
                                                }
                                            `}
                                        >
                                            <section.icon className="w-4 h-4" />
                                            {section.title}
                                        </button>

                                        {/* Sub-items */}
                                        {section.subItems.length > 0 && (
                                            <div className="ml-9 mt-1 space-y-1 border-l border-border/50 pl-2">
                                                {section.subItems.map(sub => (
                                                    <button
                                                        key={sub.id}
                                                        onClick={() => scrollToSection(sub.id)}
                                                        className="block w-full text-left px-2 py-1 text-xs text-muted-foreground hover:text-primary transition-colors"
                                                    >
                                                        {sub.title}
                                                    </button>
                                                ))}
                                            </div>
                                        )}
                                    </div>
                                ))}
                            </div>
                        </div>

                        <div className="px-4">
                            <h4 className="py-2 text-sm font-semibold text-foreground/70 uppercase tracking-wider">Resources</h4>
                            <div className="space-y-1">
                                <a href="https://npmjs.com/package/memcloud" target="_blank" rel="noreferrer" className="block px-4 py-2 text-sm text-muted-foreground hover:text-foreground">JS SDK Reference</a>
                                <a href="https://crates.io/crates/memcloud" target="_blank" rel="noreferrer" className="block px-4 py-2 text-sm text-muted-foreground hover:text-foreground">Rust Crate</a>
                            </div>
                        </div>
                    </div>
                </aside>

                {/* Backdrop for Mobile Sidebar */}
                {mobileMenuOpen && (
                    <div className="fixed inset-0 bg-black/50 z-30 md:hidden" onClick={() => setMobileMenuOpen(false)} />
                )}

                {/* Main Content */}
                <main className="flex-1 w-full max-w-4xl mx-auto px-6 py-10 md:px-12">

                    {/* Introduction */}
                    <section id="introduction" className="mb-16 scroll-mt-24">
                        <div className="flex items-center gap-3 mb-6">
                            <div className="p-2 rounded-lg bg-green-500/10 text-green-500"><Terminal className="w-6 h-6" /></div>
                            <h2 className="text-2xl font-bold">Introduction</h2>
                        </div>

                        <p className="text-lg text-muted-foreground leading-relaxed">
                            The <code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded">memcli</code> tool is your primary interface for interacting with the MemCloud daemon.
                            Manage peers, check cluster status, and perform manual storage operations for testing and scripts.
                        </p>

                        <div className="mt-8 p-4 rounded-lg bg-yellow-500/10 border border-yellow-500/20 flex gap-3 text-yellow-500">
                            <div className="mt-1"><Terminal className="w-5 h-5" /></div>
                            <div className="text-sm">
                                Ensure you have the daemon running via <code className="font-bold">memcli node start</code> or as a background service before executing commands.
                            </div>
                        </div>
                    </section>

                    {/* Installation */}
                    <section id="installation" className="mb-16 scroll-mt-24 border-t border-border/50 pt-10">
                        <div className="flex items-center gap-3 mb-6">
                            <div className="p-2 rounded-lg bg-orange-500/10 text-orange-500"><Download className="w-6 h-6" /></div>
                            <h2 className="text-2xl font-bold">Installation</h2>
                        </div>
                        <p className="text-muted-foreground mb-6">
                            Get started by installing the MemCloud binaries or the SDK for your preferred language.
                        </p>

                        <h3 id="install-script" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Direct Install (macOS/Linux)</h3>
                        <p className="text-sm text-muted-foreground mb-4">Installs the latest release binaries to your system.</p>
                        <CommandBlock
                            command="curl -fsSL https://raw.githubusercontent.com/vibhanshu2001/memcloud/main/install.sh | sh"
                            description="Downloads and executes the installation script, setting up MemCloud on your machine."
                        />

                        <h3 id="install-windows" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Direct Install (Windows)</h3>
                        <p className="text-sm text-muted-foreground mb-4">Run this in PowerShell to install.</p>
                        <CommandBlock
                            command="irm https://raw.githubusercontent.com/vibhanshu2001/memcloud/main/install.ps1 | iex"
                            description="Downloads and executes the installation script, setting up MemCloud on your machine."
                        />

                        <h3 id="install-npm" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">JavaScript / TypeScript SDK</h3>
                        <CommandBlock
                            command="npm install memcloud"
                            description="Install the client library to interact with the MemCloud daemon from your Node.js applications."
                        />
                    </section>

                    {/* Peer Management */}
                    <section id="peers" className="mb-16 scroll-mt-24 border-t border-border/50 pt-10">
                        <div className="flex items-center gap-3 mb-6">
                            <div className="p-2 rounded-lg bg-blue-500/10 text-blue-500"><Network className="w-6 h-6" /></div>
                            <h2 className="text-2xl font-bold">Peer Management</h2>
                        </div>
                        <p className="text-muted-foreground mb-6">
                            Connect and manage other MemCloud nodes in your mesh.
                        </p>

                        <h3 id="peer-connect" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Connect to a Peer</h3>
                        <CommandBlock
                            command='memcli connect <ADDR> --quota "1gb"'
                            description="Initiate a connection. Omit --quota to use interactive mode. Supports secure Noise-based handshake."
                        />

                        <h3 id="peer-list" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">List Active Peers</h3>
                        <CommandBlock
                            command="memcli peer list"
                            description="Show a table of all authenticated peers, their Node Name, stats, and RAM usage."
                        />

                        <h3 id="peer-update" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Update Peer Limits (Live)</h3>
                        <CommandBlock
                            command='memcli peer update <ID_OR_NAME> --quota "512mb"'
                            description="Dynamically adjust the memory quota for a connected peer by Name or ID."
                        />

                        <h3 id="peer-disconnect" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Disconnect Peer</h3>
                        <CommandBlock
                            command='memcli peer disconnect <ID_OR_NAME>'
                            description="Gracefully close the secure session with a specific peer."
                        />
                    </section>

                    {/* Security & Trust */}
                    <section id="security" className="mb-16 scroll-mt-24 border-t border-border/50 pt-10">
                        <div className="flex items-center gap-3 mb-6">
                            <div className="p-2 rounded-lg bg-indigo-500/10 text-indigo-500"><Shield className="w-6 h-6" /></div>
                            <h2 className="text-2xl font-bold">Security & Trust</h2>
                        </div>
                        <p className="text-muted-foreground mb-6">
                            Manage trusted devices. By default, new connections require explicit consent.
                        </p>

                        <h3 id="sec-list" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">List Trusted Devices</h3>
                        <CommandBlock
                            command="memcli trust list"
                            description="Show all devices that have been permanently trusted."
                        />

                        <h3 id="sec-consent" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Interactive Consent</h3>
                        <CommandBlock
                            command="memcli consent"
                            description="Open an interactive prompt to approve or deny pending connection requests."
                        />

                        <h3 id="sec-remove" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Remove Trusted Device</h3>
                        <CommandBlock
                            command="memcli trust remove <NAME_OR_KEY>"
                            description="Revoke trust for a specific device. Future connections will require re-approval."
                        />
                    </section>

                    {/* Storage Operations */}
                    <section id="storage" className="mb-16 scroll-mt-24 border-t border-border/50 pt-10">
                        <div className="flex items-center gap-3 mb-6">
                            <div className="p-2 rounded-lg bg-purple-500/10 text-purple-500"><Shield className="w-6 h-6" /></div>
                            <h2 className="text-2xl font-bold">Storage Operations</h2>
                        </div>
                        <p className="text-muted-foreground mb-6">
                            Manually storing and retrieving data is useful for testing scripts or simple CLI-based IPC.
                        </p>

                        <h3 id="store-text" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Store Text/Bytes</h3>
                        <CommandBlock
                            command='memcli store "Hello World" --peer <NODE_NAME>'
                            description="Uploads a string to your local node by default. Use --peer to target a specific node, or --remote to let the cluster decide."
                        />

                        <h3 id="store-kv-set" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Key-Value Set</h3>
                        <CommandBlock
                            command='memcli set app-config "{\"theme\": \"dark\"}" --peer <NODE_NAME>'
                            description="Link a block ID to a human-readable key. Supports --peer to set the key directly on a remote node."
                        />

                        <h3 id="store-kv-get" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Retrieve Data</h3>
                        <CommandBlock
                            command='memcli get app-config --peer <NODE_NAME>'
                            description="Fetch data by key. Use --peer to query a specific node, otherwise queries the whole cluster."
                        />
                    </section>

                    {/* System Commands */}
                    <section id="system" className="mb-16 scroll-mt-24 border-t border-border/50 pt-10">
                        <div className="flex items-center gap-3 mb-6">
                            <div className="p-2 rounded-lg bg-green-500/10 text-green-500"><Cpu className="w-6 h-6" /></div>
                            <h2 className="text-2xl font-bold">System Commands</h2>
                        </div>

                        <h3 id="sys-start" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Start Daemon</h3>
                        <CommandBlock
                            command='memcli node start --port 8080 --memory 1073741824'
                            description="Launch the MemCloud daemon. Recommended to run this via a service manager (systemd/launchd) in production."
                        />

                        <h3 id="sys-stream" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Streaming Input</h3>
                        <CommandBlock
                            command='cat huge.log | memcli stream'
                            description="Accepts stdin and streams it to the cluster in chunks. Perfect for data larger than local RAM."
                        />
                    </section>

                    {/* Contributing */}
                    <section id="contributing" className="mb-16 scroll-mt-24 border-t border-border/50 pt-10">
                        <div className="flex items-center gap-3 mb-6">
                            <div className="p-2 rounded-lg bg-slate-500/10 text-slate-500"><Github className="w-6 h-6" /></div>
                            <h2 className="text-2xl font-bold">Contributing</h2>
                        </div>
                        <p className="text-muted-foreground mb-6">
                            Want to modify the core Daemon or CLI? Build from source to get started.
                        </p>

                        <h3 id="contrib-source" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Build from Source</h3>
                        <p className="text-sm text-muted-foreground mb-4">Requires Rust and Cargo to be installed.</p>
                        <CommandBlock
                            command={`git clone https://github.com/vibhanshu2001/memcloud.git
cd memcloud
cargo build --release
# Binaries are now in ./target/release/memnode and ./target/release/memcli`}
                            description="Clone the repository and build the optimization-enabled binaries."
                        />

                        <h3 id="contrib-daemon" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Running the Daemon</h3>
                        <CommandBlock
                            command={`# Run memnode with debug logging
RUST_LOG=info ./target/release/memnode --name "DevNode" --port 8080

# Or use the CLI to start in background
./target/release/memcli node start --name "DevNode"`}
                            description="Start the daemon directly with environment variables or use the CLI wrapper."
                        />

                        <h3 id="contrib-cli" className="text-lg font-semibold mt-8 mb-2 scroll-mt-24">Running the CLI</h3>
                        <CommandBlock
                            command={`# Check node status
./target/release/memcli node status

# Store some data
./target/release/memcli store "Hello, MemCloud!"

# List peers
./target/release/memcli peers`}
                            description="Interact with your local build using the CLI binary."
                        />
                    </section>

                    <footer className="mt-20 pt-10 border-t border-border/50 text-center text-sm text-muted-foreground">
                        <p>MemCloud CLI v{CLI_VERSION} Documentation</p>
                    </footer>
                </main>
            </div>
        </div >
    );
};

export default CliDocs;
