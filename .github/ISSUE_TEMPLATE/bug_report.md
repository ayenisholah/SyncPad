---
name: Bug report
about: Something behaves incorrectly or crashes
title: ""
labels: bug
assignees: ""
---

## Description

A clear description of what the bug is. For sync issues (lost keystrokes,
documents diverging between windows, cursors landing in the wrong place),
please say so explicitly — those are treated as the highest severity.

## Reproduction

Steps to reproduce, ideally with two browser windows on the same document
(or a raw WebSocket client such as `websocat` against `/ws/:docId`):

1.
2.
3.

## Expected behavior

What you expected to happen.

## Actual behavior

What actually happened. Include server logs (`tracing` output) and browser
console output if relevant.

## Environment

- SyncPad commit/version:
- rustc version (if running the server):
- Browser and version:
- OS:
