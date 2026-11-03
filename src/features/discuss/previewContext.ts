import { createContext } from "react";
import type { DiscussApi } from "../../api/discuss";
export const DiscussPreviewContext = createContext<DiscussApi | null>(null);
