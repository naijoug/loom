export interface ProjectSummary {
  id: string;
  path: string;
  name: string;
  detectedStacks: string[];
  suggestedCommands: string[];
  isGitRepository: boolean;
  gitBranch?: string;
  hasUncommittedChanges: boolean;
  loomDirReady: boolean;
  schemaVersion: number;
}
