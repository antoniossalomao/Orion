import { pathToFileURL } from 'node:url';
const sdk = process.env.ORION_TEST_NODE_SDK;
const { Client } = await import(pathToFileURL(`${sdk}/dist/esm/client/index.js`));
const { StreamableHTTPClientTransport } = await import(pathToFileURL(`${sdk}/dist/esm/client/streamableHttp.js`));
async function read(token) {
    const client = new Client({ name: 'orion-export-fixture', version: '1.0.0' });
    const transport = new StreamableHTTPClientTransport(new URL(process.env.ORION_TEST_RPC), { requestInit: { headers: { Authorization: `Bearer ${token}` } } });
    try {
        await client.connect(transport);
        const result = await client.callTool({ name: 'list_facts', arguments: {} });
        const tools = await client.listTools();
        return { protocol: transport.protocolVersion, text: JSON.stringify(result), tools: tools.tools.map(t => t.name), denied: false };
    } catch (_) { return { denied: true }; }
    finally { await client.close().catch(() => {}); }
}
const results = await Promise.all([read(process.env.ORION_TEST_CLIENT_A), read(process.env.ORION_TEST_CLIENT_B)]);
process.stdout.write(JSON.stringify(results));
