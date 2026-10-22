// gemini-axi-managed: project-scoped ambient context
import { exec } from 'node:child_process';
const command = __COMMAND__;
const run = (suffix = '', input = '') => new Promise(resolve => {
  const child = exec(command + suffix, { timeout: 20000, maxBuffer: 65536 }, (_error, stdout) => resolve(stdout.trim()));
  child.stdin.end(input);
});
function context() {
  const sessions = new Map();
  return async id => {
    if (!sessions.has(id)) sessions.set(id, run());
    return await sessions.get(id);
  };
}
export default {
  id: 'gemini-axi',
  async setup(ctx) {
    const ambient = context();
    let lastSessionID;
    await ctx.session.hook('context', async event => {
      lastSessionID = event.sessionID;
      event.system.push({ type: 'text', text: await ambient(event.sessionID) });
    });
    // V2 has no SessionEnd hook. Preserve the final observed session on location unload.
    return async () => {
      if (lastSessionID) await run(' --session-end', JSON.stringify({ session_id: lastSessionID }));
    };
  },
  async server() {
    const ambient = context();
    return {
      'experimental.chat.system.transform': async (input, output) => {
        output.system.push(await ambient(input.sessionID));
      },
      event: async ({ event }) => {
        if (event.type === 'session.deleted') await run(' --session-end', JSON.stringify({ session_id: event.properties.info.id }));
      },
    };
  },
};
