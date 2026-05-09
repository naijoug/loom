export interface ProjectSummary {
  path: string;
  name: string;
  detectedStacks: string[];
  suggestedCommands: string[];
  isGitRepository: boolean;
}
