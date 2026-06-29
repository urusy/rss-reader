// Switch — Ark UI の headless Switch をトークンで装飾した薄ラップ。
// part 名・data 属性は @ark-ui/solid 5.37 / @zag-js/switch 1.41 で確認済みだが、
// メジャー更新で変わりうる。壊れたら https://ark-ui.com (Solid / Switch) で確認。
import { Switch as ArkSwitch } from "@ark-ui/solid/switch";
import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

type SwitchProps = ComponentProps<typeof ArkSwitch.Root> & { label?: string };

export function Switch(props: SwitchProps) {
  const [local, rest] = splitProps(props, ["class", "label"]);
  return (
    <ArkSwitch.Root class={cn("inline-flex items-center gap-2", local.class)} {...rest}>
      <ArkSwitch.Control
        class={cn(
          "inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full bg-input p-0.5 transition-colors",
          "data-[state=checked]:bg-primary",
          // 実フォーカスは隠し input。zag が Control に data-focus-visible を立てるのでそこにリングを当てる。
          "data-[focus-visible]:outline-none data-[focus-visible]:ring-2 data-[focus-visible]:ring-ring",
        )}
      >
        <ArkSwitch.Thumb class="h-4 w-4 rounded-full bg-background shadow-sm transition-transform data-[state=checked]:translate-x-4" />
      </ArkSwitch.Control>
      {local.label ? (
        <ArkSwitch.Label class="select-none text-sm">{local.label}</ArkSwitch.Label>
      ) : null}
      <ArkSwitch.HiddenInput />
    </ArkSwitch.Root>
  );
}
