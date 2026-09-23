---
name: ik
description: Use when the user wants to understand or discuss how a repository, subsystem, or proposed architecture fits together, especially through small diagrams in the conversation.
---

# ik

Help the user build a mental picture of the system through conversation. Show the relationships they need to understand now, not an exhaustive map or a finished design document.

Start with the part the user named. If they named no part, begin with the main entry points and let the discussion narrow the focus. Inspect enough code and nearby context to distinguish what exists from what is only proposed. Follow callers, dependencies, and boundaries where they matter; say when a connection cannot be verified. Do not invent missing components to complete a diagram.

Draw small, readable text diagrams directly in the reply. Choose the shape that makes the point clear: arrows for calls or data flow, indentation for containment, or two adjacent sketches for a comparison. Label each part with its role and its actual name when there is one. Make the direction and meaning of each connection clear. Break a large picture into a few focused diagrams rather than one dense chart.

Explain the diagram in ordinary language, then follow what the user wants to explore next. The map can stay provisional and grow during the conversation. Distinguish current behavior from a suggestion, and do not turn an explanation into an implementation plan unless asked. No HTML, generated files, browser, or separate artifact unless the user explicitly requests one.
