import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();

// start snippet define-protocol
const tomlConfig = panproto.defineProtocol({
  name: 'toml-config',
  schemaTheory: 'ThConstrainedGraph',
  instanceTheory: 'ThWType',
  edgeRules: [
    { edgeKind: 'key', srcKinds: ['section'], tgtKinds: [] },
    { edgeKind: 'subsection', srcKinds: ['section'], tgtKinds: ['section'] },
  ],
  objKinds: ['section'],
  constraintSorts: ['required', 'deprecated'],
});
// end snippet define-protocol

// start snippet use-protocol
const configSchema = tomlConfig.schema()
  .vertex('root', 'section')
  .vertex('root.database', 'section')
  .vertex('root.database.host', 'string')
  .vertex('root.database.port', 'integer')
  .edge('root', 'root.database', 'subsection', { name: 'database' })
  .edge('root.database', 'root.database.host', 'key', { name: 'host' })
  .edge('root.database', 'root.database.port', 'key', { name: 'port' })
  .constraint('root.database.host', 'required', 'true')
  .build();
// end snippet use-protocol
