import { A } from "@solidjs/router";
import { useApp } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import SidebarContent from "./SidebarContent";

/** md 未満で表示するハンバーガー + 左ドロワー（Dialog side="left"）。 */
export default function MobileTopBar() {
  const app = useApp();
  return (
    <header class="flex items-center gap-2 border-b border-border px-4 py-3 md:hidden">
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
