import { createContext } from "react";
import type { ResearchApi } from "../../api/research";
/** Only main.tsx's explicit DEV fixture branch supplies an implementation. */
export const ResearchPreviewContext = createContext<ResearchApi | null>(null);
