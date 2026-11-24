import express from 'express';
import http from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import JSZip from 'jszip';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WORKSPACE_DIR = path.resolve(__dirname, 'rust-workspace');

interface WorkspaceFile {
  path: string;
  content: string;
  bytes: number;
}

function resolvePort(): number {
  const portArgIdx = process.argv.indexOf('--port');
  if (portArgIdx !== -1 && process.argv[portArgIdx + 1]) {
    const parsed = Number(process.argv[portArgIdx + 1]);
    if (Number.isFinite(parsed) && parsed > 0) {
      return parsed;
    }
  }
  const nginxPort = process.env.NGINX_PORT;
  const envPort = process.env.PORT;
  if (envPort && envPort !== nginxPort) {
    const parsed = Number(envPort);
    if (Number.isFinite(parsed) && parsed > 0) {
      return parsed;
    }
  }
  const defaultPort = Number(process.env.DEFAULT_APP_PORT);
  if (Number.isFinite(defaultPort) && defaultPort > 0) {
    return defaultPort;
  }
  return 3000;
}

async function collectWorkspaceFiles(dir: string, baseDir: string): Promise<WorkspaceFile[]> {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const files: WorkspaceFile[] = [];

  for (const entry of entries) {
    if (entry.name === 'target' || entry.name === 'node_modules') {
      continue;
    }
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collectWorkspaceFiles(fullPath, baseDir)));
    } else if (entry.isFile()) {
      const relPath = path.relative(baseDir, fullPath).split(path.sep).join('/');
      const content = await fs.readFile(fullPath, 'utf8');
      files.push({
        path: relPath,
        content,
        bytes: Buffer.byteLength(content, 'utf8'),
      });
    }
  }

  return files.sort((a, b) => a.path.localeCompare(b.path));
}

async function startServer() {
  const app = express();
  const httpServer = http.createServer(app);
  const port = resolvePort();

  app.get('/api/workspace-files', async (_req, res) => {
    try {
      const files = await collectWorkspaceFiles(WORKSPACE_DIR, WORKSPACE_DIR);
      res.json({ files });
    } catch (err) {
      res.status(500).json({ error: err instanceof Error ? err.message : 'Failed to read workspace' });
    }
  });

  app.get('/api/workspace.zip', async (_req, res) => {
    try {
      const files = await collectWorkspaceFiles(WORKSPACE_DIR, WORKSPACE_DIR);
      const zip = new JSZip();
      for (const file of files) {
        zip.file(`forja3d-workspace/${file.path}`, file.content);
      }
      const buffer = await zip.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE' });
      res.setHeader('Content-Type', 'application/zip');
      res.setHeader('Content-Disposition', 'attachment; filename="forja3d-rust-workspace.zip"');
      res.send(buffer);
    } catch (err) {
      res.status(500).json({ error: err instanceof Error ? err.message : 'Failed to package workspace' });
    }
  });

  if (process.env.NODE_ENV !== 'production') {
    const { createServer: createViteServer } = await import('vite');
    const vite = await createViteServer({
      server: {
        middlewareMode: true,
        hmr: process.env.DISABLE_HMR === 'true' ? false : { server: httpServer },
      },
      appType: 'spa',
    });
    app.use(vite.middlewares);
  } else {
    const distDir = path.resolve(__dirname, 'dist');
    app.use(express.static(distDir));
    app.get('*', (_req, res) => {
      res.sendFile(path.join(distDir, 'index.html'));
    });
  }

  httpServer.listen(port, '0.0.0.0', () => {
    console.log(`Forja3D server listening on http://0.0.0.0:${port}`);
  });
}

startServer();
