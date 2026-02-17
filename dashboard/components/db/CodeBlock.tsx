"use client";

import CodeEditor from "./CodeEditor";

interface CodeBlockProps {
  code: string;
  language?: "yaml" | "json" | "sql";
  maxHeight?: string;
  className?: string;
}

/**
 * Read-only syntax-highlighted code block.
 *
 * Thin wrapper around CodeEditor with readOnly=true, no active line
 * highlighting, and compact sizing suitable for inline display in cards.
 */
export default function CodeBlock({
  code,
  language = "json",
  maxHeight = "240px",
  className,
}: CodeBlockProps) {
  return (
    <CodeEditor
      value={code}
      language={language}
      readOnly
      minHeight="60px"
      maxHeight={maxHeight}
      className={className}
    />
  );
}
