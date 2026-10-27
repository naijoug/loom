import { createContext } from "react";
import type { LearnApi } from "../../api/learn";
/** Only main.tsx's explicit DEV fixture branch supplies an implementation. */
export const LearnPreviewContext = createContext<LearnApi | null>(null);
