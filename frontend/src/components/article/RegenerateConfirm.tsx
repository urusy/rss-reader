import { buttonVariants } from "@/components/ui/button";
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogCloseTrigger,
} from "@/components/ui/dialog";

/**
 * 再要約/再翻訳の確認ダイアログ（誤タップ1回で Claude を呼び直してトークンを
 * 消費しないためのガード）。初回生成には使わない — キャッシュを破棄して
 * 作り直すときだけこのボタンを出す。ArticleDetail の DeleteConfirm と同型。
 */
export default function RegenerateConfirm(props: {
  /** 対象の名前（「要約」「翻訳」）。ダイアログ文面に使う。 */
  label: string;
  /** トリガーボタンの表示（例: 再要約 (Claude)）。busy 中は busyText。 */
  trigger: string;
  busyText: string;
  busy: boolean;
  disabled: boolean;
  variant: "default" | "outline";
  onConfirm: () => void;
}) {
  return (
    <Dialog>
      <DialogTrigger
        class={buttonVariants({ size: "sm", variant: props.variant })}
        disabled={props.disabled}
      >
        {props.busy ? props.busyText : props.trigger}
      </DialogTrigger>
      <DialogContent>
        <DialogTitle>{props.label}を作り直しますか？</DialogTitle>
        <DialogDescription>
          キャッシュ済みの{props.label}を破棄して Claude
          を呼び直します（トークンを消費します）。
        </DialogDescription>
        <div class="mt-4 flex justify-end gap-2">
          <DialogCloseTrigger
            class={buttonVariants({ size: "sm", variant: "outline" })}
          >
            キャンセル
          </DialogCloseTrigger>
          <DialogCloseTrigger
            class={buttonVariants({ size: "sm" })}
            onClick={() => props.onConfirm()}
          >
            作り直す
          </DialogCloseTrigger>
        </div>
      </DialogContent>
    </Dialog>
  );
}
