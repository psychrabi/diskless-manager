import { cn } from "@/lib/utils";

const LogViewer = ({ content, emptyText = "(no logs yet)", className = "" }) => (
  <pre
    className={cn(
      "max-h-[calc(100vh-19rem)] overflow-auto whitespace-pre-wrap break-words rounded-xl border border-white/8 bg-[#06110f] p-4 font-mono-data text-[11px] leading-5 text-[#b8d8d0] shadow-inner selection:bg-primary/30 selection:text-white",
      className
    )}
  >
    {content || emptyText}
  </pre>
);

export default LogViewer;
