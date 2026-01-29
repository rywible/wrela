const fs = require('fs');
const path = require('path');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

let client;

function resolveServerCommand() {
  const config = vscode.workspace.getConfiguration('wrela');
  const configuredCommand = config.get('languageServer.command');
  const configuredArgs = config.get('languageServer.args') || [];

  if (configuredCommand && configuredCommand.trim().length > 0) {
    return { command: configuredCommand, args: configuredArgs };
  }

  const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
  if (workspaceFolder) {
    const workspacePath = workspaceFolder.uri.fsPath;
    const localBinary = path.join(workspacePath, 'target', 'debug', 'wrela-lsp');
    if (fs.existsSync(localBinary)) {
      return { command: localBinary, args: [], cwd: workspacePath };
    }

    const cargoToml = path.join(workspacePath, 'Cargo.toml');
    if (fs.existsSync(cargoToml)) {
      return {
        command: 'cargo',
        args: ['run', '-p', 'wrela-lsp', '--quiet'],
        cwd: workspacePath,
      };
    }
  }

  return { command: 'wrela-lsp', args: [] };
}

function activate(context) {
  const { command, args, cwd } = resolveServerCommand();
  const serverOptions = {
    command,
    args,
    options: cwd ? { cwd } : undefined,
  };
  const clientOptions = {
    documentSelector: [{ scheme: 'file', language: 'wrela' }],
  };

  client = new LanguageClient('wrela', 'Wrela LSP', serverOptions, clientOptions);
  context.subscriptions.push(client.start());
}

function deactivate() {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

module.exports = {
  activate,
  deactivate,
};
