---
name: commit
description: Use when I ask to create a Git commit or draft a commit message, including when the result I ask for is a commit without saying so. Not for other outgoing text.
---

# commit

Commit only after I approve the final message.

1. **Draft early when work is still ahead.** When I ask for work plus a commit, show a provisional message from the intended result before you start, in a `text` code block. Say it is provisional: it is not the approval. Skip this when the changes already exist.

2. **Know what "this" is.** Read `git status --short` and `git diff --cached --stat`. When there is nothing to commit, say so and stop. Keep any staging scope I named. When this session made the changes, "commit this" means them: stage them, with `git add -A` when they are the only changes, by exact paths or patches otherwise. In a fresh session with no such changes, show me the coherent groups and ask which. When staged work or overlapping edits cannot be separated from the intended ones, stop and say what cannot be separated.

3. **Read the context**: the branch, the last 10 messages (`git log -10 --pretty=format:"%h%n%B"`), and the full staged diff.

4. **Write the message.** The first line is a short subject about the change, with nothing before it. Match the style of recent commits: tense, capitals, prefixes such as `fix:`. Add a body only when it says something the subject does not. Check a provisional message against what is really staged and change it where they differ. No emojis, no filler, no diff retold line by line.

5. **Ask with the full message in the question.** The question text is the complete message, never a preview or a pointer to text above, because chat text and previews do not always show. Options: Confirm, and I can't see the full message. I ask for changes or cancel in the free text field; when the tool has none, add Request changes and Cancel.

6. **Handle the answer.** "I can't see the full message": write it in the chat, then ask again. Changes: redraft and ask again. Cancel: stop. Run `git commit` only after Confirm.

7. **Commit and read it back.** Commit with the approved message (`git commit -F -`). Read it with `git log -1 --pretty=format:"%B"` and apply "Text that leaves" from the global instructions. Amend only to remove what that rule removes, and say what you removed.

8. **Push.** When the goal's agreed side effects include pushing this commit, push. Otherwise ask through the question tool. Use `git push`, or `git push -u origin <branch>` when the branch has no upstream.
