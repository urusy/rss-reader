import SidebarContent from "./SidebarContent";

/** デスクトップ用の永続 aside（lg 以上で表示。md〜lg 未満や iPad 縦はドロワーに委ねる）。 */
export default function Sidebar() {
  return (
    <aside class="sticky top-0 hidden h-dvh overflow-y-auto border-r border-border pb-safe lg:flex lg:flex-col">
      <SidebarContent />
    </aside>
  );
}
