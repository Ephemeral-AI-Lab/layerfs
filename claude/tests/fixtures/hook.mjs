// Hook unit tests inject deployment readiness; real Docker/FUSE is owner testing.
import { dispatch } from '../../scripts/hook.mjs';
let text=''; for await (const chunk of process.stdin) text+=chunk;
console.log(JSON.stringify(await dispatch(JSON.parse(text),{ deploymentCheck:()=>{
  if (process.env.LFS_TEST_DEPLOY_FAIL) throw new Error('deployment unavailable');
} })));
