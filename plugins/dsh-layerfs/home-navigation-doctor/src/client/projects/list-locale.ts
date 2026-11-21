/** Copy of the sidebar's LayerFS project list (namespace `layerfsProjectList`). */
export const NS = 'layerfsProjectList'
const en = {
  configure: 'Set up {project}', kindSqlite: 'SQLite', kindPg: 'PostgreSQL', search: 'Search projects', noMatch: 'No project matches.', colStore: 'Store', openProject: 'Open project', hide: 'Hide from sidebar',
  emptyRecent: 'No project sessions yet', newProject: 'New project',
  newSessionMenu: 'New session or project', newSessionIn: 'New session in', otherProject: 'Other project…', importFolder: 'Import a folder…', importTitle: 'Import a folder', cancel: 'Cancel', close: 'Close', openFailed: 'Could not start the session: {message}',
  available: 'Available', stopped: 'Stopped', unavailable: 'Unavailable', checking: 'Checking…', starting: 'Starting…', busy: 'Busy',
  archiveSession: 'Archive {session}', archiveRefused: 'Not archived', archiveAgain: 'Try again', expandProject: 'Toggle conversations in {project}', archiveLoading: 'Checking archived conversations…', archiveFailed: 'Archived conversations unavailable: {message}',
  panel: 'Projects', layerfs: 'LayerFS Projects', openStorage: 'Open Storage', showLayerfs: 'Show LayerFS projects', showDsh: 'Show DSH workspaces',
  loading: 'Loading projects…', timedOut: 'The read timed out. Try again.', empty: 'No LayerFS projects yet', retry: 'Retry',
  failed: 'Could not load projects: {message}', sessionsFailed: 'Could not load sessions: {message}',
  untitled: 'New session', newSession: 'New session in {project}', cannotRun: 'Can’t start a session here: {reason}',
  noSessions: 'No sessions yet',
  actionFailed: 'Could not open the project session. Check the project’s machine, then retry.', machines: 'Machines', details: 'Details',
  interrupted: 'Interrupted', interruptedHint: 'Execution was interrupted. History is kept; a new message re-activates it after access returns.',
  machineStopped: 'Machine stopped', machineUnavailable: 'Machine unavailable', storeUnavailable: 'Store unavailable',
  notVerified: 'Not verified', removed: 'Removed', replaced: 'Replaced',
  foldCenter: 'Fold sidebar with LayerFS Center open', reviewStart: 'Review the machine before starting it.', openingSession: 'Opening a session…',
  start: 'Start', startNamed: 'Start {machine}', startFailed: 'Could not start the project’s machine.', startMachineFor: 'Start machine {machine} for {project}',
}
const zh: typeof en = {
  configure: '设置 {project}', kindSqlite: 'SQLite', kindPg: 'PostgreSQL', search: '搜索项目', noMatch: '没有匹配的项目。', colStore: '存储', openProject: '打开项目', hide: '从侧边栏隐藏',
  emptyRecent: '还没有项目会话', newProject: '新建项目',
  newSessionMenu: '新建会话或项目', newSessionIn: '新建会话于', otherProject: '其他项目…', importFolder: '导入文件夹…', importTitle: '导入文件夹', cancel: '取消', close: '关闭', openFailed: '无法开始会话：{message}',
  available: '可用', stopped: '已停止', unavailable: '不可用', checking: '正在检查…', starting: '正在启动…', busy: '处理中',
  archiveSession: '归档 {session}', archiveRefused: '未归档', archiveAgain: '重试', expandProject: '展开或收起 {project} 中的对话', archiveLoading: '正在检查归档对话…', archiveFailed: '归档对话不可用：{message}',
  panel: '项目', layerfs: 'LayerFS 项目', openStorage: '打开存储', showLayerfs: '显示 LayerFS 项目', showDsh: '显示 DSH 工作区',
  loading: '正在加载项目…', timedOut: '读取超时，请重试。', empty: '还没有 LayerFS 项目', retry: '重试',
  failed: '无法加载项目：{message}', sessionsFailed: '无法加载会话：{message}',
  untitled: '新会话', newSession: '在 {project} 中新建会话', cannotRun: '无法在此新建会话：{reason}',
  noSessions: '还没有会话',
  actionFailed: '无法打开项目会话。请检查项目所在的机器后重试。', machines: '机器', details: '详情',
  interrupted: '已中断', interruptedHint: '执行已中断。历史记录已保留；访问恢复后，发送新消息即可重新激活。',
  machineStopped: '机器已停止', machineUnavailable: '机器不可用', storeUnavailable: '存储不可用',
  notVerified: '未验证', removed: '已移除', replaced: '已被替换',
  foldCenter: '保持 LayerFS 中心打开并折叠侧栏', reviewStart: '请先核对要启动的机器。', openingSession: '正在打开会话…',
  start: '启动', startNamed: '启动 {machine}', startFailed: '无法启动项目所在的机器。', startMachineFor: '为 {project} 启动机器 {machine}',
}
export const locales = { en, zh }
export type ListT = (key: keyof typeof en, params?: Record<string, string | number>) => string
