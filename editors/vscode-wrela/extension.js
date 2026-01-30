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
  ensureWrelaTokenColors();
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
  context.subscriptions.push(
    vscode.commands.registerCommand('wrela.codeLens.showReferences', () => {
      vscode.window.showInformationMessage('Use "Find All References" for details.');
    })
  );
  context.subscriptions.push(
    vscode.commands.registerCommand('wrela.goToTypeDefinitionClient', () => {
      requestTypeDefinition('wrela.goToTypeDefinition');
    })
  );
  context.subscriptions.push(
    vscode.commands.registerCommand('wrela.peekTypeDefinitionClient', () => {
      requestTypeDefinition('wrela.peekTypeDefinition');
    })
  );
  context.subscriptions.push(
    vscode.commands.registerCommand('wrela.smokeTest.openGuide', async () => {
      const guide = vscode.Uri.joinPath(context.extensionUri, 'SMOKE_TEST.md');
      await vscode.commands.executeCommand('markdown.showPreview', guide);
    })
  );
}

async function requestTypeDefinition(command) {
  if (!client) {
    return;
  }
  await client.onReady();
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  const uri = editor.document.uri.toString();
  const pos = editor.selection.active;
  const result = await client.sendRequest('workspace/executeCommand', {
    command,
    arguments: [uri, pos.line, pos.character],
  });
  if (!Array.isArray(result) || result.length === 0) {
    vscode.window.showInformationMessage('Type definition not found.');
    return;
  }
  const locations = result.map((loc) => {
    const locUri = vscode.Uri.parse(loc.uri);
    const range = new vscode.Range(
      loc.range.start.line,
      loc.range.start.character,
      loc.range.end.line,
      loc.range.end.character
    );
    return new vscode.Location(locUri, range);
  });
  if (command === 'wrela.peekTypeDefinition') {
    await vscode.commands.executeCommand(
      'editor.action.peekLocations',
      editor.document.uri,
      pos,
      locations,
      'peek'
    );
  } else {
    await vscode.commands.executeCommand(
      'editor.action.goToLocations',
      editor.document.uri,
      pos,
      locations,
      'goto'
    );
  }
}

function ensureWrelaTokenColors() {
  const config = vscode.workspace.getConfiguration();
  const existing = config.get('editor.tokenColorCustomizations') || {};
  const rules = Array.isArray(existing.textMateRules) ? existing.textMateRules.slice() : [];
  const desired = [
    {
      scope: 'punctuation.separator.wrela',
      settings: { foreground: '#D4D4D4' },
    },
  ];
  let changed = false;
  const byScope = new Map(rules.map((rule) => [rule.scope, rule]));
  for (const rule of desired) {
    const existingRule = byScope.get(rule.scope);
    if (!existingRule) {
      rules.push(rule);
      byScope.set(rule.scope, rule);
      changed = true;
      continue;
    }
    const existingSettings = existingRule.settings || {};
    if (existingSettings.foreground !== rule.settings.foreground) {
      existingRule.settings = { ...existingSettings, foreground: rule.settings.foreground };
      changed = true;
    }
  }
  if (!changed) {
    return;
  }
  const updated = { ...existing, textMateRules: rules };
  config.update(
    'editor.tokenColorCustomizations',
    updated,
    vscode.ConfigurationTarget.Workspace
  );
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
