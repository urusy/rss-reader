// Dialog — wraps Ark UI's headless dialog and styles it with our Tailwind tokens.
//
// Why Ark UI (not shadcn-solid/Kobalte): as of 2026-06, shadcn-solid and Kobalte
// have been stagnant for ~11-15 months, whereas Ark UI (@ark-ui/solid, zag.js
// based) ships every few weeks. Ark UI is headless, so the visual identity stays
// identical to the rest of the app via app.css tokens.
//
// NOTE: Ark UI's compound API can change across major versions. If an import or
// part name breaks, check https://ark-ui.com (Solid / Dialog) for the current shape.
//
// Usage:
//   import {
//     Dialog, DialogTrigger, DialogContent, DialogTitle,
//     DialogDescription, DialogCloseTrigger,
//   } from "@/components/ui/dialog";
//   import { Button } from "@/components/ui/button";
//
//   <Dialog>
//     <DialogTrigger as={Button}>削除</DialogTrigger>
//     <DialogContent>
//       <DialogTitle>フィードを削除しますか？</DialogTitle>
//       <DialogDescription>この操作は取り消せません。</DialogDescription>
//       <div class="mt-4 flex justify-end gap-2">
//         <DialogCloseTrigger as={Button} variant="outline">キャンセル</DialogCloseTrigger>
//         <Button variant="destructive" onClick={onConfirm}>削除する</Button>
//       </div>
//     </DialogContent>
//   </Dialog>

import { Dialog as ArkDialog } from "@ark-ui/solid/dialog";
import { Portal } from "solid-js/web";
import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

export const Dialog = ArkDialog.Root;
export const DialogTrigger = ArkDialog.Trigger;
export const DialogCloseTrigger = ArkDialog.CloseTrigger;

/** Backdrop + centered, bordered content panel, rendered in a Portal. */
export function DialogContent(props: ComponentProps<typeof ArkDialog.Content>) {
  const [local, rest] = splitProps(props, ["class", "children"]);
  return (
    <Portal>
      <ArkDialog.Backdrop class="fixed inset-0 z-50 bg-black/50" />
      <ArkDialog.Positioner class="fixed inset-0 z-50 flex items-center justify-center p-4">
        <ArkDialog.Content
          class={cn(
            "w-full max-w-md rounded-lg border border-border bg-background p-6 shadow-lg",
            local.class,
          )}
          {...rest}
        >
          {local.children}
        </ArkDialog.Content>
      </ArkDialog.Positioner>
    </Portal>
  );
}

export function DialogTitle(props: ComponentProps<typeof ArkDialog.Title>) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <ArkDialog.Title
      class={cn("text-lg font-semibold tracking-tight", local.class)}
      {...rest}
    />
  );
}

export function DialogDescription(props: ComponentProps<typeof ArkDialog.Description>) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <ArkDialog.Description
      class={cn("mt-1 text-sm text-muted-foreground", local.class)}
      {...rest}
    />
  );
}
