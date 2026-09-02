import { cn } from "@/lib/utils";

const LogViewer = ({ content, emptyText = "(no logs yet)", className = "" }) => (
  <pre
    className={cn(
      "bg-base-300 p-2 rounded overflow-auto text-xs whitespace-pre-wrap max-h-[calc(100vh-20rem)]",
      className
    )}
  >
    {content || emptyText}
  </pre>
);

export default LogViewer;