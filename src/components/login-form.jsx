import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";

// Adapted from shadcn login-03 for the server's username/password authentication.
export function LoginForm({ className, onSubmit, register, errors, isSubmitting, canBootstrap, onCreateAdmin }) {
  return (
    <Card className={cn("w-full shadow-sm", className)}>
      <CardHeader className="gap-2 px-6 pt-2 text-center">
        <h1 className="text-2xl font-semibold tracking-tight">Welcome back</h1>
        <CardDescription>Sign in to manage your diskless boot server.</CardDescription>
      </CardHeader>
      <CardContent className="px-6 pb-2">
        <form onSubmit={onSubmit}>
          <FieldGroup>
            <Field data-invalid={!!errors.username}>
              <FieldLabel htmlFor="username">Username</FieldLabel>
              <Input id="username" autoComplete="username" placeholder="Enter your username" className="h-10" aria-invalid={!!errors.username} aria-describedby={errors.username ? "username-error" : undefined} {...register("username")} />
              <FieldError id="username-error">{errors.username?.message}</FieldError>
            </Field>
            <Field data-invalid={!!errors.password}>
              <FieldLabel htmlFor="password">Password</FieldLabel>
              <Input id="password" type="password" autoComplete="current-password" placeholder="Enter your password" className="h-10" aria-invalid={!!errors.password} aria-describedby={errors.password ? "password-error" : undefined} {...register("password")} />
              <FieldError id="password-error">{errors.password?.message}</FieldError>
            </Field>
            <Field>
              <Button type="submit" className="h-10" disabled={isSubmitting}>
                {isSubmitting && <Spinner />}
                {isSubmitting ? "Signing in…" : "Sign in"}
              </Button>
            </Field>
            {canBootstrap && (
              <Field className="border-t pt-5">
                <p className="text-center text-sm text-muted-foreground">No administrator account exists yet.</p>
                <Button type="button" variant="outline" onClick={onCreateAdmin}>Create your first admin account</Button>
              </Field>
            )}
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
