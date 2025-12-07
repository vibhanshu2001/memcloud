import { useState } from "react";
import { Check, Copy } from "lucide-react";

interface TerminalProps {
  command: string;
  className?: string;
}

export const Terminal = ({ command, className = "" }: TerminalProps) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className={`bg-terminal-bg border border-terminal-border rounded-lg overflow-hidden ${className}`}>
      <div className="flex items-center gap-2 px-4 py-2 border-b border-terminal-border">
        <div className="w-3 h-3 rounded-full bg-red-500/80" />
        <div className="w-3 h-3 rounded-full bg-yellow-500/80" />
        <div className="w-3 h-3 rounded-full bg-green-500/80" />
        <span className="ml-2 text-xs text-muted-foreground font-mono">terminal</span>
      </div>
      <div className="p-4 flex items-start justify-between gap-4">
        <code className="text-sm md:text-base font-mono text-foreground text-left break-all">
          <span className="text-muted-foreground">$ </span>
          {command}
        </code>
        <button
          onClick={handleCopy}
          className="p-2 hover:bg-secondary rounded-md transition-colors text-muted-foreground hover:text-foreground shrink-0"
          aria-label="Copy command"
        >
          {copied ? <Check className="w-4 h-4 text-code-green" /> : <Copy className="w-4 h-4" />}
        </button>
      </div>
    </div>
  );
};
