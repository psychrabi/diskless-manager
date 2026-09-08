import { useEffect, useId, useRef } from "react";
import { AlertTriangle, ChevronDown, Home, RefreshCw, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";
import { getErrorMessage, reloadApplication, returnToHome } from "@/lib/error-recovery";

// Does not depend on the router, authentication, theme, or toast providers.
export default function ErrorFallback({
  error, componentStack, onRetry, onHome = returnToHome, fullPage = true,
  title, description, showDetails = import.meta.env.DEV,
}) {
  const heading = useRef(null);
  const headingId = useId();
  const message = getErrorMessage(error);
  const isModuleError = /dynamically imported module|importing a module script failed|loading chunk .*failed/i.test(message);
  useEffect(() => { heading.current?.focus(); }, []);

  return (
    <section aria-labelledby={headingId} className={cn("flex w-full items-center justify-center p-4 sm:p-6", fullPage ? "min-h-svh bg-muted/40" : "min-h-[24rem]")}>
      <Card className="w-full max-w-lg shadow-sm">
        <CardHeader className="items-center gap-3 px-6 pt-3 text-center">
          <div className="flex size-12 items-center justify-center rounded-xl bg-destructive/10 text-destructive"><AlertTriangle className="size-6" /></div>
          <h1 id={headingId} ref={heading} tabIndex={-1} className="text-2xl font-semibold tracking-tight outline-none">
            {title || (isModuleError ? "This page could not load" : "Something went wrong")}
          </h1>
          <CardDescription className="max-w-sm leading-relaxed">
            {description || (isModuleError
              ? "Part of the application could not be loaded. Reload the application to try again."
              : "We couldn’t display this part of the application. Try again, or return to the dashboard to continue.")}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-5 px-6 pb-3">
          <div className="flex flex-wrap items-center justify-center gap-2">
            {onRetry && !isModuleError && <Button onClick={onRetry}><RotateCcw />Try again</Button>}
            <Button variant={isModuleError || !onRetry ? "default" : "outline"} onClick={reloadApplication}><RefreshCw />Reload application</Button>
            <Button variant="ghost" onClick={onHome}><Home />Go to dashboard</Button>
          </div>
          {showDetails && error != null && (
            <Collapsible className="rounded-lg border bg-muted/30">
              <CollapsibleTrigger className="group flex w-full items-center justify-between gap-2 rounded-lg px-3 py-2 text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring">
                Error details<ChevronDown className="size-4 transition-transform group-data-open:rotate-180" />
              </CollapsibleTrigger>
              <CollapsibleContent>
                <pre className="max-h-64 overflow-auto border-t p-3 text-xs whitespace-pre-wrap wrap-anywhere">{message}{componentStack ? `\n\n${componentStack}` : ""}</pre>
              </CollapsibleContent>
            </Collapsible>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
