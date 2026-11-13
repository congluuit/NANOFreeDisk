// Typed bridge to "Similar photos" (src-tauri/src/commands/similar.rs).

import { call } from "../api";
import type { OperationResult } from "../clean/types";
import type { Preset, SimilarAction, SimilarFindOptions, SimilarReport, SimilarSelection } from "./types";

export const similarApi = {
  /** Search `paths`, the personal folders with `options.personal`, or the Analyze scan when `paths` is empty. */
  find: (paths: string[], options: SimilarFindOptions) => call<SimilarReport>("similar_find", { paths, options }),
  /** Group the last search again for another preset (nothing is read again). */
  regroup: (preset: Preset) => call<SimilarReport>("similar_regroup", { preset }),
  cancel: () => call<void>("similar_cancel"),
  report: () => call<SimilarReport | null>("similar_report"),
  run: (selections: SimilarSelection[], action: SimilarAction) => call<OperationResult>("similar_run", { selections, action }),
  reveal: (path: string) => call<void>("similar_reveal", { path }),
  /** A small preview for the grid, or with `large` the picture for the side-by-side view. */
  thumbnail: (path: string, large = false) => call<string | null>("similar_thumbnail", { path, large }),
};
