import { A } from "@solidjs/router";
import { useApp } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import SidebarContent from "./SidebarContent";

/** lg 未満で表示するハンバーガー + 左ドロワー（Dialog side="left"）。iPad 縦も含む。 */
export default function MobileTopBar() {
  const app = useApp();
  return (
    <header class="flex items-center gap-2 border-b border-border pb-3 pl-[calc(1rem+env(safe-area-inset-left))] pr-[calc(1rem+env(safe-area-inset-right))] pt-[calc(0.75rem+env(safe-area-inset-top))] lg:hidden">
      <Dialog
        open={app.state.sidebarOpen}
        onOpenChange={(d) => (d.open ? app.openSidebar() : app.closeSidebar())}
      >
        <Button
          variant="ghost"
          size="icon"
          onClick={() => app.openSidebar()}
          aria-label="メニュー"
        >
          ≡
        </Button>
        <DialogContent side="left">
          <SidebarContent onNavigate={app.closeSidebar} />
        </DialogContent>
      </Dialog>
      <A href="/" class="text-lg font-semibold tracking-tight">
        RSS Reader
      </A>
    </header>
  );
}
