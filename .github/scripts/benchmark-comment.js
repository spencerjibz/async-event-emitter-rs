const MARKER = '<!-- benchmark-comparison -->';

module.exports = async ({ github, context, issueNumber, body }) => {
     if (!Number.isInteger(issueNumber) || issueNumber <= 0) {
          throw new Error('invalid pull request number');
     }
     if (!body.startsWith(MARKER)) {
          throw new Error('unexpected comment body');
     }
     body = body.slice(0, 60000);

     const comments = await github.paginate(github.rest.issues.listComments, {
          ...context.repo,
          issue_number: issueNumber,
     });
     const existing = comments.find(
          (c) => c.user.login === 'github-actions[bot]' && c.body.startsWith(MARKER),
     );
     if (existing) {
          await github.rest.issues.updateComment({ ...context.repo, comment_id: existing.id, body });
     } else {
          await github.rest.issues.createComment({ ...context.repo, issue_number: issueNumber, body });
     }
};
