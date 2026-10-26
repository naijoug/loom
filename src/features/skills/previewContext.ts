import { createContext } from "react";
import type { SkillsApi } from "../../api/skills";
/** Only main.tsx's explicit DEV fixture branch supplies an implementation. */
export const SkillsPreviewContext = createContext<SkillsApi | null>(null);
