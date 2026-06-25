import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

export function Card(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <div
      class={cn(
        "rounded-lg border border-border bg-card text-card-foreground shadow-sm",
        local.class,
      )}
      {...rest}
    />
  );
}

export function CardHeader(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return <div class={cn("flex flex-col space-y-1.5 p-4", local.class)} {...rest} />;
}

export function CardTitle(props: ComponentProps<"h3">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <h3 class={cn("font-semibold leading-none tracking-tight", local.class)} {...rest} />
  );
}

export function CardContent(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return <div class={cn("p-4 pt-0", local.class)} {...rest} />;
}
