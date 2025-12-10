import { useState, useEffect, useCallback, useRef } from "react";
import { Play, RotateCcw, ArrowLeft, ArrowRight, Copy, Check } from "lucide-react";

interface CommandStep {
  command: string;
  output: string[];
  delay?: number;
}

const commands: CommandStep[] = [
  {
    command: 'curl -fsSL https://raw.githubusercontent.com/vibhanshu2001/memcloud/main/install.sh | sh',
    output: [
      '╔══════════════════════════════════════════════════════════════╗',
      '║                                                              ║',
      '║           ☁  M E M C L O U D   I N S T A L L E R  ⚡          ║',
      '║                                                              ║',
      '║   "Turning nearby devices into your personal RAM farm."     ║',
      '║                                                              ║',
      '╚══════════════════════════════════════════════════════════════╝',
      '',
      '➤ Initializing MemCloud deployment sequence...',
      '➤ Scanning system parameters...',
      '➤ Fetching latest MemCloud release metadata...',
      '➤ Latest version detected: v0.1.4',
      '',
      '➤ Preparing download for Darwin (arm64)',
      '➤ Downloading MemCloud v0.1.4 package...',
      '  100% ████████████████████████ 2573k/2573k',
      '',
      '➤ Extracting payload...',
      '➤ Deploying binaries to /usr/local/bin...',
      '',
      '✔ MemCloud successfully installed! 🚀',
      '',
      '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━',
      '  ✨ You\'re Ready to Begin:',
      '     Start daemon:   memcli node start --name "MyDevice"',
      '     Check status:   memcli node status',
      '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━',
      '',
      'Welcome to the Distributed Memory Future. ✨',
    ],
    delay: 2000,
  },
  {
    command: 'memcli node start --name "MacBookPro" --port 8080',
    output: [
      '🚀 Starting MemCloud node "MacBookPro" on port 8080...',
      '✅ Node started successfully (PID: 12345)',
      '📡 mDNS discovery enabled',
    ],
    delay: 1000,
  },
  {
    command: 'memcli connect 192.168.1.11:8081',
    output: [
      '🔗 Initiating connection to 192.168.1.11:8081...',
      '✅ Connection established!',
      '🔐 Secure Session Established (Noise XX / ChaCha20-Poly1305)',
      '📡 Handshake successful (Node ID: UbuntuServer)',
      '   Latency: 1.2ms | Bandwidth: 1.0 Gbps',
    ],
    delay: 1200,
  },
  {
    command: 'memcli peers',
    output: [
      '┌──────────────┬─────────────────┬────────┐',
      '│ Node         │ Address         │ RAM    │',
      '├──────────────┼─────────────────┼────────┤',
      '│ MacBookPro   │ 192.168.1.10    │ 4.2 GB │',
      '│ UbuntuServer │ 192.168.1.15    │ 8.0 GB │',
      '│ LinuxDesktop │ 192.168.1.22    │ 2.8 GB │',
      '└──────────────┴─────────────────┴────────┘',
      '📊 Total pooled RAM: 15.0 GB',
    ],
    delay: 1200,
  },
  {
    command: 'memcli stream logs/access.log --compress',
    output: [
      '🌊 Streaming [access.log] to MemCloud cluster...',
      '⚙️  Configuration: Gzip Compression | Strategy: Distributed',
      '',
      '⠋ Processing stream... ',
      '  25% [====>                 ] 250MB | RSS: 112MB',
      '  50% [=========>            ] 500MB | RSS: 114MB',
      '  75% [===============>      ] 750MB | RSS: 118MB',
      '  100% [====================>] 1.0GB | RSS: 129MB',
      '',
      '✅ Stream Complete! (Block ID: 55892441163469)',
      '📉 Compression Ratio: 8.4x (Stored 121MB)',
    ],
    delay: 1500,
  },
  {
    command: 'memcli stats',
    output: [
      '╭─────────────────────────────────────╮',
      '│       MemCloud Cluster Stats        │',
      '├─────────────────────────────────────┤',
      '│ Nodes Online:     3                 │',
      '│ Total RAM Pool:   15.0 GB           │',
      '│ Used:             1.12 GB (7.4%)    │',
      '│ Avg Latency:      2.3ms             │',
      '│ Blocks Stored:    12,403            │',
      '╰─────────────────────────────────────╯',
    ],
    delay: 1500,
  },
  {
    command: 'memcli flush --all',
    output: [
      '⚠️  Warning: This will delete ALL data from the cluster.',
      'Auto-confirming (demo mode)...',
      '',
      '🧹 Flushing cluster memory...',
      '   • Node [MacBookPro]: Freed 129MB',
      '   • Node [UbuntuServer]: Freed 992MB',
      '',
      '✅ Cluster flushed. 1.12 GB reclaimed.',
    ],
    delay: 1000,
  }
];

export const InteractiveTerminal = () => {
  const [currentStep, setCurrentStep] = useState(0);
  const [typedCommand, setTypedCommand] = useState("");
  const [showOutput, setShowOutput] = useState(false);
  const [isTyping, setIsTyping] = useState(false);
  const [isComplete, setIsComplete] = useState(false);
  const [hasStarted, setHasStarted] = useState(true);
  const [visibleLines, setVisibleLines] = useState<string[]>([]);
  const timeoutsRef = useRef<NodeJS.Timeout[]>([]);
  const [isPlaying, setIsPlaying] = useState(true);
  const [isCopied, setIsCopied] = useState(false);

  // Helper to clear all pending timeouts
  const clearAllTimeouts = useCallback(() => {
    timeoutsRef.current.forEach(clearTimeout);
    timeoutsRef.current = [];
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => clearAllTimeouts();
  }, [clearAllTimeouts]);

  const typeCommand = useCallback((command: string, onComplete: () => void) => {
    setIsTyping(true);
    setTypedCommand("");
    let index = 0;

    const typeChar = () => {
      // Check if we're still playing/valid to proceed? 
      // Actually timeouts are cleared on state change, so this closure won't execute if cleared/cancelled?
      // No, existing timeouts fire unless cleared. clearAllTimeouts handles that.

      if (index < command.length) {
        setTypedCommand(command.slice(0, index + 1));
        index++;
        const t = setTimeout(typeChar, 30 + Math.random() * 40);
        timeoutsRef.current.push(t);
      } else {
        setIsTyping(false);
        onComplete();
      }
    };

    const t = setTimeout(typeChar, 300);
    timeoutsRef.current.push(t);
  }, []);

  const streamOutput = useCallback((lines: string[], onComplete: () => void) => {
    setVisibleLines([]);
    let index = 0;

    const showLine = () => {
      if (index < lines.length) {
        setVisibleLines(prev => [...prev, lines[index]]);
        index++;
        // Variable speed based on line length to simulate "processing"
        const delay = 50 + Math.random() * 50;
        const t = setTimeout(showLine, delay);
        timeoutsRef.current.push(t);
      } else {
        onComplete();
      }
    };

    showLine();
  }, []);

  // One-step execution logic
  const runStep = useCallback(() => {
    clearAllTimeouts();

    // Safety check
    if (currentStep >= commands.length) {
      // Loop back logic invoked by the step incrementer, but if we land here:
      // Reset to 0 and continue playing
      const t = setTimeout(() => {
        setCurrentStep(0);
        setTypedCommand("");
        setShowOutput(false);
        setVisibleLines([]);
      }, 2000); // Pause before restarting loop
      timeoutsRef.current.push(t);
      return;
    }

    const step = commands[currentStep];

    // 1. Type Command
    typeCommand(step.command, () => {
      // 2. Wait a bit
      const t1 = setTimeout(() => {
        setShowOutput(true);
        // 3. Stream Output
        streamOutput(step.output, () => {
          // 4. Wait before next step
          const t2 = setTimeout(() => {
            // Advance to next step
            setCurrentStep(prev => {
              const next = prev + 1;
              if (next >= commands.length) {
                // End of loop, wait then reset (handled by effect triggering again on step change??)
                // Actually if we set it to length, the effect will see length and Trigger the Reset logic above.
                return next;
              }
              return next;
            });
            setTypedCommand("");
            setShowOutput(false);
            setVisibleLines([]);
          }, step.delay || 2000);
          timeoutsRef.current.push(t2);
        });
      }, 400);
      timeoutsRef.current.push(t1);
    });
  }, [currentStep, typeCommand, streamOutput, clearAllTimeouts]);

  // Effect to drive auto-play
  useEffect(() => {
    if (hasStarted && isPlaying) {
      runStep();
    }
    // IMPORTANT: Cleanup timeouts when dependencies change (e.g. user clicks Next, pausing play)
    // so we don't have rogue timeouts fighting.
    return () => clearAllTimeouts();
  }, [hasStarted, isPlaying, currentStep, runStep, clearAllTimeouts]);


  const handlePrev = () => {
    setIsPlaying(false); // Pause auto-play
    clearAllTimeouts();

    if (currentStep > 0) {
      setCurrentStep(prev => prev - 1);
      // Reset visual state
      setTypedCommand("");
      setShowOutput(false);
      setVisibleLines([]);
      setIsTyping(false);
    }
  };

  const handleNext = () => {
    setIsPlaying(false); // Pause auto-play
    clearAllTimeouts();

    if (currentStep < commands.length - 1) {
      setCurrentStep(prev => prev + 1);
      // Reset visual state
      setTypedCommand("");
      setShowOutput(false);
      setVisibleLines([]);
      setIsTyping(false);
    }
  };

  const handleStart = () => {
    setHasStarted(true);
    setIsPlaying(true);
    setCurrentStep(0);
    setTypedCommand("");
    setShowOutput(false);
    setVisibleLines([]);
    // Effect will pick this up
  };

  // Resume auto-play if user wants? Or just Play button turns into Pause?
  // User didn't ask for pause button, just "back and forth nav".
  // Note: Once paused (by clicking prev/next), the user sees the static command for that step (empty output initially).
  // They probably want to see the completed state for that step?
  // Current logic: typedCommand is empty. That's bad for manual nav.
  // Fix for manual navigation: Show the FULL command and Output immediately?
  // Or just let them hit "Play" again?

  // Let's make manual nav show full content immediately so it's useful.
  useEffect(() => {
    if (!isPlaying && hasStarted) {
      // If paused, just show full content of current step
      if (currentStep < commands.length) {
        setTypedCommand(commands[currentStep].command);
        setShowOutput(true);
        setVisibleLines(commands[currentStep].output);
        setIsTyping(false);
      }
    }
  }, [isPlaying, hasStarted, currentStep]);


  // Determine OS
  const [os, setOs] = useState<'mac' | 'windows' | 'linux'>('mac');
  useEffect(() => {
    if (typeof window !== 'undefined') {
      const userAgent = window.navigator.userAgent.toLowerCase();
      if (userAgent.includes('mac')) {
        setOs('mac');
      } else if (userAgent.includes('win')) {
        setOs('windows');
      } else if (userAgent.includes('linux')) {
        setOs('linux');
      } else {
        setOs('mac');
      }
    }
  }, []);

  const handleRestart = () => {
    clearAllTimeouts();
    setHasStarted(false);
    setCurrentStep(0);
    setTypedCommand("");
    setShowOutput(false);
    setVisibleLines([]);
    setIsTyping(false);
    setIsComplete(false);
    setIsPlaying(false); // Ensure playing state is reset
  };

  const currentCommand = commands[currentStep];

  return (
    <div className="bg-terminal-bg border border-terminal-border rounded-xl overflow-hidden w-full shadow-2xl text-left font-mono">
      {/* Re-implementing Header with proper layout for OS themes */}
      <div className="relative flex items-center justify-between px-4 py-3 border-b border-terminal-border bg-secondary/30 min-h-[46px]">
        {/* Left Side */}
        <div className="flex items-center gap-2 z-10 w-[100px]">
          {os === 'mac' && (
            <>
              <div className="w-3 h-3 rounded-full bg-[#FF5F56] hover:brightness-90 transition-all shadow-sm" />
              <div className="w-3 h-3 rounded-full bg-[#FFBD2E] hover:brightness-90 transition-all shadow-sm" />
              <div className="w-3 h-3 rounded-full bg-[#27C93F] hover:brightness-90 transition-all shadow-sm" />
            </>
          )}
          {os !== 'mac' && (
            <span className="text-xs text-muted-foreground/70 font-sans tracking-wide">
              {os === 'windows' ? 'C:\\WINDOWS\\system32\\cmd.exe' : 'vibhanshu@ubuntu:~'}
            </span>
          )}
        </div>

        {/* Center Title (Mac only usually, or simplified) */}
        {os === 'mac' && (
          <div className="absolute inset-x-0 mx-auto text-center w-fit">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground/60 font-medium">
              <span className="opacity-50">📁</span>
              <span>memcloud-demo</span>
            </div>
          </div>
        )}

        {/* Right Side - Actions & Win Controls */}
        <div className="flex items-center gap-3 z-10">
          {/* Navigation & Actions */}
          <div className="flex items-center gap-2">
            {hasStarted && (
              <div className="flex items-center gap-1 bg-background/50 rounded-md p-0.5 border border-white/5 mr-2">
                <button
                  onClick={handlePrev}
                  disabled={currentStep === 0}
                  className="p-1 hover:bg-muted text-muted-foreground disabled:opacity-30 transition-colors rounded-sm group relative"
                  title="Previous Step"
                >
                  <ArrowLeft className="w-3.5 h-3.5" />
                  {/* Tooltip or status? */}
                </button>
                <div className="w-[1px] h-3 bg-border/50" />
                <button
                  onClick={handleNext}
                  disabled={currentStep >= commands.length - 1}
                  className="p-1 hover:bg-muted text-muted-foreground disabled:opacity-30 transition-colors rounded-sm"
                  title="Next Step"
                >
                  <ArrowRight className="w-3.5 h-3.5" />
                </button>
                <div className="w-[1px] h-3 bg-border/50 ml-1" />
                <button
                  onClick={() => setIsPlaying(!isPlaying)}
                  className="p-1 hover:bg-muted text-muted-foreground transition-colors rounded-sm ml-1"
                  title={isPlaying ? "Pause" : "Play"}
                >
                  {isPlaying ? <span className="block w-2.5 h-2.5 bg-current rounded-sm text-yellow-500/80" /> : <Play className="w-3 h-3 text-green-500" />}
                </button>
              </div>
            )}

            {/* Run Demo Logic Removed (Auto-Starts) */}
            <div className="w-[1px] h-3 bg-border/50 ml-1" />
            <button
              onClick={() => {
                navigator.clipboard.writeText(commands[currentStep].command);
                setIsCopied(true);
                setTimeout(() => setIsCopied(false), 2000);
              }}
              className="p-1 hover:bg-muted text-muted-foreground transition-colors rounded-sm ml-1"
              title="Copy Command"
            >
              {isCopied ? <Check className="w-3.5 h-3.5 text-green-500" /> : <Copy className="w-3.5 h-3.5" />}
            </button>
          </div>

          {/* Windows Controls */}
          {os !== 'mac' && (
            <div className="flex items-center gap-3 pl-3 border-l border-white/10">
              <div className="w-2.5 h-2.5 border-b-2 border-muted-foreground" />
              <div className="w-2.5 h-2.5 border border-muted-foreground rounded-[1px]" />
              <div className="relative w-3 h-3">
                <div className="absolute top-1/2 left-0 w-full h-[2px] bg-muted-foreground rotate-45 transform" />
                <div className="absolute top-1/2 left-0 w-full h-[2px] bg-muted-foreground -rotate-45 transform" />
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Terminal content - Fixed Height with Scroll */}
      <div className="h-[450px] overflow-y-auto p-4 font-mono text-sm text-left bg-terminal-bg scrollbar-thin scrollbar-thumb-white/10 scrollbar-track-transparent">
        <div>
          <div className="space-y-4">
            {/* Current step content */}
            {currentStep < commands.length && (
              <div className="text-left w-full">
                <div className="flex items-start gap-2 text-left">
                  <span className="text-code-green select-none shrink-0">❯</span>
                  <span className="text-foreground break-all text-left">
                    {typedCommand}
                    {isTyping && (
                      <span className="inline-block w-2 h-4 bg-primary ml-0.5 animate-pulse" />
                    )}
                  </span>
                </div>
                {showOutput && (
                  <div className="mt-2 pl-4 space-y-0.5 animate-fade-in text-left">
                    {visibleLines.map((line, lineIndex) => (
                      <p
                        key={lineIndex}
                        className="text-muted-foreground text-xs leading-relaxed whitespace-pre-wrap break-words text-left"
                      >
                        {line}
                      </p>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Progress indicator */}
      {hasStarted && (
        <div className="px-4 py-2 border-t border-terminal-border bg-secondary/20">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            {/* Fix step counter: Cap at commands.length */}
            <span>Step {Math.min(currentStep + 1, commands.length)} of {commands.length}</span>
            <div className="flex gap-1.5">
              {commands.map((_, index) => (
                <button
                  key={index}
                  onClick={() => {
                    setIsPlaying(false); // Manual selection pauses
                    clearAllTimeouts();
                    setCurrentStep(index);
                  }}
                  className={`w-2 h-2 rounded-full transition-all duration-300 ${index < currentStep
                    ? "bg-code-green hover:bg-code-green/80"
                    : index === currentStep
                      ? "bg-primary scale-125 animate-pulse"
                      : "bg-muted hover:bg-muted-foreground"
                    }`}
                  aria-label={`Go to step ${index + 1}`}
                />
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
