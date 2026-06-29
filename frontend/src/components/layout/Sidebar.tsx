import SidebarContent from "./SidebarContent";

/** デスクトップ用の永続 aside（md 以上で表示）。 */
export default function Sidebar() {
  return (
    <aside class="sticky top-0 hidden h-screen overflow-y-auto border-r border-border md:flex md:flex-col">
      <SidebarContent />
    </aside>
  );
}
