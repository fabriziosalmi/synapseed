import * as assert from 'assert';
import {
    kvItem, sectionItem, loadingItem, errorItem, emptyItem,
    progressItem, gradeIcon, severityIcon, clickableItem, fileItem,
} from '../../items';

suite('kvItem', () => {
    test('creates item with label and description', () => {
        const item = kvItem('Label', 'Description', 'symbol-class');
        assert.strictEqual(item.label, 'Label');
        assert.strictEqual(item.description, 'Description');
        assert.ok(item.tooltip);
    });
});

suite('sectionItem', () => {
    test('creates collapsible parent with children count', () => {
        const children = [kvItem('A', '1'), kvItem('B', '2')];
        const section = sectionItem('Section', children, 'folder');
        assert.strictEqual(section.label, 'Section');
        assert.strictEqual(section.description, '(2)');
        assert.strictEqual(section.children?.length, 2);
        // TreeItemCollapsibleState.Expanded = 2
        assert.strictEqual(section.collapsibleState, 2);
    });

    test('creates collapsed section when requested', () => {
        const children = [kvItem('A', '1')];
        const section = sectionItem('Section', children, 'folder', true);
        // TreeItemCollapsibleState.Collapsed = 1
        assert.strictEqual(section.collapsibleState, 1);
    });

    test('creates non-collapsible when no children', () => {
        const section = sectionItem('Empty', [], 'folder');
        // TreeItemCollapsibleState.None = 0
        assert.strictEqual(section.collapsibleState, 0);
    });
});

suite('Factory helpers', () => {
    test('loadingItem defaults to Loading...', () => {
        assert.strictEqual(loadingItem().label, 'Loading...');
    });

    test('loadingItem accepts custom text', () => {
        assert.strictEqual(loadingItem('Please wait').label, 'Please wait');
    });

    test('errorItem shows Error label', () => {
        const item = errorItem('boom');
        assert.strictEqual(item.label, 'Error');
        assert.strictEqual(item.description, 'boom');
    });

    test('emptyItem uses the message as label', () => {
        assert.strictEqual(emptyItem('No data').label, 'No data');
    });

    test('progressItem shows percentage', () => {
        const item = progressItem('Health', 85);
        assert.ok((item.description as string).includes('85%'));
    });

    test('clickableItem sets command', () => {
        const item = clickableItem('Click', 'desc', 'my.command', ['arg1']);
        assert.strictEqual(item.command?.command, 'my.command');
        assert.deepStrictEqual(item.command?.arguments, ['arg1']);
    });

    test('fileItem creates item with resourceUri', () => {
        const item = fileItem('File', '/path/to/file.rs', 10);
        assert.ok(item.resourceUri);
        assert.strictEqual(item.contextValue, 'synapseed.fileLink');
        assert.strictEqual(item.command?.command, 'vscode.open');
    });
});

suite('Icon helpers', () => {
    test('gradeIcon maps A-F', () => {
        assert.strictEqual(gradeIcon('A'), 'pass');
        assert.strictEqual(gradeIcon('B'), 'info');
        assert.strictEqual(gradeIcon('C'), 'warning');
        assert.strictEqual(gradeIcon('D'), 'error');
        assert.strictEqual(gradeIcon('F'), 'error');
    });

    test('severityIcon maps severity strings', () => {
        assert.strictEqual(severityIcon('error'), 'error');
        assert.strictEqual(severityIcon('Error'), 'error');
        assert.strictEqual(severityIcon('warning'), 'warning');
        assert.strictEqual(severityIcon('WARN'), 'warning');
        assert.strictEqual(severityIcon('info'), 'info');
        assert.strictEqual(severityIcon('note'), 'info');
    });
});
