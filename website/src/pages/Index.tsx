import { TermDefinition } from "@/components/TermDefinition";
import {
  Cloud,
  Zap,
  Wifi,
  Server,
  Terminal as TerminalIcon,
  WifiOff,
  Github,
  ExternalLink,
  ArrowRight
} from "lucide-react";
import { Terminal } from "@/components/Terminal";
import { InteractiveTerminal } from "@/components/InteractiveTerminal";
import { FeatureCard } from "@/components/FeatureCard";
import { CodeBlock } from "@/components/CodeBlock";
import { Badge } from "@/components/Badge";

const features = [
  {
    icon: Server,
    title: "Distributed RAM Pooling",
    description: "Combine idle RAM from multiple devices on your LAN into a shared, ephemeral storage cloud.",
  },
  {
    icon: Wifi,
    title: "Zero-Config Discovery",
    description: "Automatic peer discovery via mDNS — no manual IP setup required.",
  },
  {
    icon: Zap,
    title: "Millisecond Latency",
    description: "Store and load data across devices in < 10ms on your local network.",
  },
  {
    icon: Cloud,
    title: "Multi-Device Support",
    description: "Works seamlessly with macOS, Ubuntu, and other Linux distributions.",
  },
  {
    icon: WifiOff,
    title: "Works Offline",
    description: "Fully functional over LAN without any internet connection.",
  },
  {
    icon: TerminalIcon,
    title: "CLI + SDKs",
    description: "Command-line interface, Rust SDK, and TypeScript/JS SDK included.",
  },
];

const codeExample = `import {MemCloud} from 'memcloud';

    const cloud = new MemCloud();
    await cloud.connect();

    // Store data across your network
    const handle = await cloud.store("My Data");

    // Key-Value operations
    await cloud.set("config", JSON.stringify({theme: "dark" }));
    const config = await cloud.get("config");`;

const Index = () => {
  return (
    <div className="min-h-screen bg-background">
      {/* Background gradient */}
      <div className="fixed inset-0 bg-gradient-to-b from-primary/5 via-transparent to-transparent pointer-events-none" />

      {/* Header */}
      <header className="relative z-10 border-b border-border/50">
        <div className="container mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Cloud className="w-6 h-6 text-primary" />
            <span className="font-semibold text-lg">MemCloud</span>
          </div>
          <div className="flex items-center gap-4">
            <a
              href="https://github.com/vibhanshu2001/memcloud/blob/main/ARCHITECTURE.md"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors mr-2"
            >
              <Server className="w-4 h-4" />
              <span className="hidden sm:inline">Architecture</span>
            </a>
            <a
              href="https://github.com/vibhanshu2001/memcloud"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
            >
              <Github className="w-4 h-4" />
              <span className="hidden sm:inline">GitHub</span>
            </a>
            <a
              href="https://www.npmjs.com/package/memcloud"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:bg-primary/90 transition-colors"
            >
              <span>npm install</span>
              <ArrowRight className="w-4 h-4" />
            </a>
          </div>
        </div>
      </header>

      {/* Hero Section */}
      <section className="relative z-10 container mx-auto px-4 pt-20 pb-16 md:pt-32 md:pb-24">
        <div className="max-w-3xl mx-auto text-center">
          <div className="flex items-center justify-center gap-3 mb-6 animate-fade-in">
            <Badge>
              <span className="w-2 h-2 rounded-full bg-code-green animate-pulse" />
              Rust-Powered
            </Badge>
            <Badge variant="outline">MIT License</Badge>
          </div>

          <h1 className="text-4xl md:text-6xl font-bold mb-6 animate-fade-in-up">
            <span className="text-gradient">MemCloud</span>
            <span className="ml-3 text-foreground">☁️</span>
          </h1>

          <p className="text-xl md:text-2xl text-muted-foreground mb-4 animate-fade-in-delay-1">
            <TermDefinition
              term="Distributed in-memory data store"
              definition="A system that stores data across the RAM of multiple devices on your network for extremely high-speed access."
            /> written in Rust
          </p>

          <p className="text-lg text-foreground/80 mb-10 animate-fade-in-delay-2">
            "Turning nearby devices into your personal RAM farm."
          </p>

          <div className="max-w-2xl mx-auto animate-fade-in-delay-3">
            <InteractiveTerminal />
          </div>
        </div>
      </section>

      {/* Features Grid */}
      <section className="relative z-10 container mx-auto px-4 py-16 md:py-24">
        <div className="text-center mb-12">
          <h2 className="text-2xl md:text-3xl font-bold mb-4">Key Features</h2>
          <p className="text-muted-foreground max-w-lg mx-auto">
            Everything you need to <TermDefinition term="pool RAM" definition="Aggregate unused memory from multiple computers into a single shared storage capacity." /> across your local network
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 max-w-5xl mx-auto">
          {features.map((feature, index) => (
            <FeatureCard key={index} {...feature} />
          ))}
        </div>
      </section>

      {/* Interactive Terminal Section Removed (Moved to Hero) */}

      {/* Code Example */}
      <section className="relative z-10 container mx-auto px-4 py-16 md:py-24">
        <div className="max-w-3xl mx-auto">
          <div className="text-center mb-10">
            <h2 className="text-2xl md:text-3xl font-bold mb-4">Simple API</h2>
            <p className="text-muted-foreground">
              Get started with just a few lines of code
            </p>
          </div>



          <div className="mb-6 flex flex-wrap gap-4 justify-center">
            <Terminal command="npm install memcloud" className="flex-1 min-w-[280px]" />
          </div>
          <CodeBlock code={codeExample} language="typescript" />
        </div>
      </section>

      {/* Comparison */}
      <section className="relative z-10 container mx-auto px-4 py-16 md:py-24">
        <div className="max-w-4xl mx-auto">
          <div className="text-center mb-12">
            <h2 className="text-2xl md:text-3xl font-bold mb-4">Why MemCloud?</h2>
            <p className="text-muted-foreground max-w-lg mx-auto">
              Unlike Redis or Memcached, MemCloud pools idle RAM across multiple local machines
            </p>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full border-collapse">
              <thead>
                <tr className="border-b border-border">
                  <th className="text-left py-4 px-4 font-semibold text-foreground">Feature</th>
                  <th className="text-center py-4 px-4 font-semibold text-primary">MemCloud</th>
                  <th className="text-center py-4 px-4 font-semibold text-muted-foreground">Redis</th>
                  <th className="text-center py-4 px-4 font-semibold text-muted-foreground">Memcached</th>
                </tr>
              </thead>
              <tbody className="text-sm">
                <tr className="border-b border-border/50">
                  <td className="py-4 px-4 text-muted-foreground">Architecture</td>
                  <td className="py-4 px-4 text-center text-code-green font-mono">
                    <TermDefinition term="P2P / Mesh" definition="Peer-to-Peer network where nodes connect directly to share resources, eliminating the need for a central server." />
                  </td>
                  <td className="py-4 px-4 text-center text-muted-foreground">Client-Server</td>
                  <td className="py-4 px-4 text-center text-muted-foreground">Client-Server</td>
                </tr>
                <tr className="border-b border-border/50">
                  <td className="py-4 px-4 text-muted-foreground">Discovery</td>
                  <td className="py-4 px-4 text-center text-code-green font-mono">
                    <TermDefinition term="mDNS Auto" definition="Multicast DNS automatically detects other MemCloud instances on your local network without configuration." />
                  </td>
                  <td className="py-4 px-4 text-center text-muted-foreground">Manual</td>
                  <td className="py-4 px-4 text-center text-muted-foreground">Manual</td>
                </tr>
                <tr className="border-b border-border/50">
                  <td className="py-4 px-4 text-muted-foreground">Persistence</td>
                  <td className="py-4 px-4 text-center text-code-yellow font-mono">
                    <TermDefinition term="Ephemeral" definition="Data exists only in RAM and is cleared when the process stops. Optimized for speed, not long-term storage." />
                  </td>
                  <td className="py-4 px-4 text-center text-muted-foreground">
                    <TermDefinition term="RDB/AOF" definition="Redis uses RDB snapshots and Append Only Files (AOF) to persist data to disk." />
                  </td>
                  <td className="py-4 px-4 text-center text-muted-foreground">None</td>
                </tr>
                <tr>
                  <td className="py-4 px-4 text-muted-foreground">Ideal Use</td>
                  <td className="py-4 px-4 text-center text-code-blue font-mono">
                    <TermDefinition term="Local Dev/ML" definition="Perfect for local development, ML model weight loading, and temporary data sharing during training." />
                  </td>
                  <td className="py-4 px-4 text-center text-muted-foreground">Session Store</td>
                  <td className="py-4 px-4 text-center text-muted-foreground">String Cache</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </section>

      {/* CTA */}
      <section className="relative z-10 container mx-auto px-4 py-16 md:py-24">
        <div className="max-w-2xl mx-auto text-center">
          <h2 className="text-2xl md:text-3xl font-bold mb-6">Ready to pool your RAM?</h2>
          <p className="text-muted-foreground mb-8">
            Join developers using MemCloud for local caching, ML workflows, and team task distribution.
          </p>
          <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
            <a
              href="https://github.com/vibhanshu2001/memcloud"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 px-6 py-3 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition-colors glow-sm animate-pulse-glow"
            >
              <Github className="w-5 h-5" />
              View on GitHub
            </a>
            <a
              href="https://github.com/vibhanshu2001/memcloud#readme"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 px-6 py-3 bg-secondary text-secondary-foreground rounded-lg font-medium hover:bg-secondary/80 transition-colors"
            >
              Read Documentation
              <ExternalLink className="w-4 h-4" />
            </a>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="relative z-10 border-t border-border/50">
        <div className="container mx-auto px-4 py-8">
          <div className="flex flex-col md:flex-row items-center justify-between gap-4">
            <div className="flex items-center gap-2 text-muted-foreground">
              <Cloud className="w-5 h-5" />
              <span className="text-sm">MemCloud — MIT License</span>
            </div>
            <div className="flex items-center gap-6">
              <a
                href="https://github.com/vibhanshu2001/memcloud"
                target="_blank"
                rel="noopener noreferrer"
                className="text-sm text-muted-foreground hover:text-foreground transition-colors"
              >
                GitHub
              </a>
              <a
                href="https://www.npmjs.com/package/memcloud"
                target="_blank"
                rel="noopener noreferrer"
                className="text-sm text-muted-foreground hover:text-foreground transition-colors"
              >
                npm
              </a>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
};

export default Index;
