import { createContext } from "react";
import type { ResearchApi } from "../../api/research";
/** Only main.tsx's explicit DEV fixture branch supplies an implementation. */
export const ResearchPreviewContext = createContext<ResearchApi | null>(null);
export type TargetPreviewApi = typeof import("../../api/researchTargets").targetResearchApi;
/** Set only for the DEV `researchFixture=targets` single-shot surface. */
export const ResearchTargetPreviewContext = createContext<TargetPreviewApi | null>(null);
