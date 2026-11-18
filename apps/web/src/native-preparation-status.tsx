import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';
export type PreparationView={requestId:string;taskId:number;state:{status:string;outcome?:{status:string;result?:{qualification:string;target:{capabilities:unknown}};error?:{message:string}}}};
export function preparationStatus(value:PreparationView){
 const outcome=value.state.outcome;
 if(outcome?.error?.message==='native_build_original_outcome_unknown_no_automatic_retry')return {label:tr("Original preparation outcome unknown"),detail:tr("The original task has ended, but its preparation outcome is unknown. Keep the original request and check its evidence. Do not automatically rebuild or repeat the import.")};
 if(value.state.status!=='terminal')return {label:tr("Preparation task is not finished"),detail:tr("This is the last observed task status, not proof that it is still running or that an artifact is ready. You can check its progress without changing it.")};
 if(outcome?.status==='completed')return {label:tr("Artifact preparation verified"),detail:tr("The build or import capability check is complete. The artifact still needs to be selected explicitly. This does not connect an account, certify strategy compatibility, or authorize trading.")};
 if(outcome?.status==='aborted')return {label:tr("Preparation task stopped"),detail:tr("A stopped task does not prove that user code and child processes have exited or that no external effects occurred. Keep the original request for verification.")};
 const reasons:Record<string,string>={native_preparation_required_file_unavailable:tr("A required file is missing or inaccessible. Check the artifact, toolchain, and project paths visible to the service."),native_preparation_file_access_denied:tr("The service cannot read or execute a required file."),native_preparation_identity_mismatch:tr("The artifact's declared version, source, or precision does not match the requested identity."),native_preparation_capabilities_invalid:tr("The artifact's capability response does not match the required interface.")};
 const reason=outcome?.error?.message&&reasons[outcome.error.message];
 if(typeof reason==='string')return {label:tr("Artifact not yet verified"),detail:reason+tr(" Keep the original request. This does not prove that nothing ran or that no effects occurred, and it will not retry automatically.")};
 return {label:tr("Artifact not yet verified"),detail:tr("This result is not a usable artifact. Check the original preparation record and private diagnostics; do not assume that nothing ran or that no effects occurred.")};
}
export function PreparationStatus({value}:{value:PreparationView}){
  useTranslation('core');
 const status=preparationStatus(value);
 return <section aria-label={tr("Original artifact preparation status")}><h4>{status.label}</h4><p>{status.detail}</p><dl><dt>{tr("Original request")}</dt><dd>{value.requestId}</dd><dt>{tr("Original task")}</dt><dd>{value.taskId}</dd></dl></section>;
}
export function SelectedRunnerStatus({runner}:{runner:ProcessNativeTarget}){
  useTranslation('core');
 const raw=runner.capabilities.identity;
 const identity=raw&&typeof raw==='object'&&!Array.isArray(raw)?raw:{};
 const version=typeof identity.nautilusVersion==='string'&&/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(identity.nautilusVersion)&&identity.nautilusVersion.length<=80?identity.nautilusVersion:null;
 const precision=identity.precision==='high'||identity.precision==='standard'?identity.precision:null;
 return <section aria-label={tr("Explicitly selected artifact")}><h4>{tr("An artifact is explicitly selected")}</h4><dl><dt>{tr("Nautilus version")}</dt><dd>{version??tr("Not reported")}</dd><dt>{tr("Precision")}</dt><dd>{precision??tr("Not reported")}</dd><dt>{tr("Account entry point compiled")}</dt><dd>{runner.capabilities.account.compiled?tr("Included"):tr("Not included")}</dd><dt>{tr("Owner account strategy registration")}</dt><dd>{runner.capabilities.account.registration?tr("Included"):tr("Not included")}</dd><dt>{tr("Configured backtest factory")}</dt><dd>{runner.capabilities.research.configuredRunFactory?tr("Included"):tr("Not included")}</dd></dl></section>;
}
