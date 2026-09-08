import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";

// The signup block is used only for the existing first-administrator setup flow.
export function SignupForm({ onSubmit, register, errors, isSubmitting, onLogin }) {
  return (
    <Card className="w-full shadow-sm">
      <CardHeader className="gap-2 px-6 pt-2 text-center">
        <h1 className="text-2xl font-semibold tracking-tight">Set up your workspace</h1>
        <CardDescription>Create the first administrator account for your server.</CardDescription>
      </CardHeader>
      <CardContent className="px-6 pb-2">
        <form onSubmit={onSubmit}>
          <FieldGroup>
            <Field data-invalid={!!errors.username}>
              <FieldLabel htmlFor="username">Username</FieldLabel>
              <Input id="username" autoComplete="username" className="h-10" placeholder="admin" aria-invalid={!!errors.username} aria-describedby={errors.username ? "username-error" : undefined} {...register("username")} />
              <FieldError id="username-error">{errors.username?.message}</FieldError>
            </Field>
            <Field data-invalid={!!errors.password}>
              <FieldLabel htmlFor="password">Password</FieldLabel>
              <Input id="password" type="password" autoComplete="new-password" className="h-10" aria-invalid={!!errors.password} aria-describedby="password-help password-error" {...register("password")} />
              <FieldDescription id="password-help">Use at least 8 characters, including an uppercase letter, a lowercase letter, and a number.</FieldDescription>
              <FieldError id="password-error">{errors.password?.message}</FieldError>
            </Field>
            <Field data-invalid={!!errors.confirmPassword}>
              <FieldLabel htmlFor="confirmPassword">Confirm password</FieldLabel>
              <Input id="confirmPassword" type="password" autoComplete="new-password" className="h-10" aria-invalid={!!errors.confirmPassword} aria-describedby={errors.confirmPassword ? "confirm-password-error" : undefined} {...register("confirmPassword")} />
              <FieldError id="confirm-password-error">{errors.confirmPassword?.message}</FieldError>
            </Field>
            <Field>
              <Button type="submit" className="h-10" disabled={isSubmitting}>
                {isSubmitting && <Spinner />}
                {isSubmitting ? "Creating account…" : "Create admin account"}
              </Button>
              <Button type="button" variant="ghost" onClick={onLogin}>Back to sign in</Button>
            </Field>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
