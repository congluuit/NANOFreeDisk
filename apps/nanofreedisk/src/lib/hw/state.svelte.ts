// Hardware view state shared by its tabs and the Home temperature card.

import type { HwInventory } from "./types";

export type HwTab = "overview" | "sensors" | "tasks";

export const hw = $state({
  tab: "overview" as HwTab,
  inventory: null as HwInventory | null,
});
