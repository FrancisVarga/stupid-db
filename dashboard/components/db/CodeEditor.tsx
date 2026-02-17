"use client";

import { useMemo, useRef } from "react";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { yaml } from "@codemirror/lang-yaml";
import { json } from "@codemirror/lang-json";
import { sql } from "@codemirror/lang-sql";
import { tokyoNight } from "@uiw/codemirror-theme-tokyo-night";
import { EditorView, keymap } from "@codemirror/view";

type Language = "yaml" | "json" | "sql";

interface CodeEditorProps {
  value: string;
  onChange?: (value: string) => void;
  language?: Language;
  /** Minimum editor height in pixels. */
  minHeight?: string;
  /** Maximum editor height in pixels (enables scrolling). */
  maxHeight?: string;
  readOnly?: boolean;
  /** Fired on Ctrl+Enter / Cmd+Enter — useful for SQL execute. */
  onSubmit?: () => void;
  placeholder?: string;
  className?: string;
}

const languageExtensions = {
  yaml: () => yaml(),
  json: () => json(),
  sql: () => sql(),
};

/**
 * Syntax-highlighted code editor using CodeMirror 6.
 *
 * Supports YAML, JSON, and SQL with the Tokyo Night theme
 * that matches the dashboard's dark aesthetic.
 */
export default function CodeEditor({
  value,
  onChange,
  language = "yaml",
  minHeight = "200px",
  maxHeight = "500px",
  readOnly = false,
  onSubmit,
  placeholder,
  className,
}: CodeEditorProps) {
  const ref = useRef<ReactCodeMirrorRef>(null);

  // Stable callback ref for submit handler
  const submitRef = useRef(onSubmit);
  submitRef.current = onSubmit;

  const extensions = useMemo(() => {
    const exts = [];
    exts.push(languageExtensions[language]());

    // Custom theme overrides for dashboard styling
    exts.push(
      EditorView.theme({
        "&": {
          fontSize: "12px",
          borderRadius: "8px",
          border: "1px solid rgba(51, 65, 85, 0.3)",
        },
        "&.cm-focused": {
          outline: "none",
          border: "1px solid rgba(0, 240, 255, 0.3)",
        },
        ".cm-gutters": {
          background: "rgba(6, 10, 16, 0.8)",
          borderRight: "1px solid rgba(51, 65, 85, 0.2)",
        },
        ".cm-activeLineGutter": {
          background: "rgba(0, 240, 255, 0.05)",
        },
        ".cm-activeLine": {
          background: "rgba(0, 240, 255, 0.03)",
        },
      })
    );

    // Ctrl+Enter / Cmd+Enter → onSubmit
    if (onSubmit) {
      exts.push(
        keymap.of([
          {
            key: "Mod-Enter",
            run: () => {
              submitRef.current?.();
              return true;
            },
          },
        ])
      );
    }

    return exts;
  }, [language, onSubmit]);

  return (
    <div className={className}>
      <CodeMirror
        ref={ref}
        value={value}
        onChange={onChange}
        theme={tokyoNight}
        extensions={extensions}
        readOnly={readOnly}
        editable={!readOnly}
        placeholder={placeholder}
        basicSetup={{
          lineNumbers: true,
          foldGutter: true,
          bracketMatching: true,
          highlightActiveLine: !readOnly,
          highlightActiveLineGutter: !readOnly,
          indentOnInput: true,
          autocompletion: false,
        }}
        minHeight={minHeight}
        maxHeight={maxHeight}
      />
    </div>
  );
}
