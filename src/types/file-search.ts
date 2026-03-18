/** Fuzzy file search result matching the Rust FileMatch struct. */
export interface FileMatch {
  score: number;
  path: string;
  root: string;
  indices?: number[];
}
