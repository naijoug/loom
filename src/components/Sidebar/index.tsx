import { Brand } from "./Brand";
import { Navigation } from "./Navigation";

export function Sidebar({ showTasks = true }: { showTasks?: boolean }) {
  return (
    <>
      <Brand />
      <Navigation showTasks={showTasks} />
    </>
  );
}
