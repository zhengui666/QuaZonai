// Generated from Rust OpenAPI. Do not edit.
"use strict";
exports.response0 = validate20;
const schema31 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1artifacts/get/responses/200/content/application~1json/schema"};
const schema33 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"access_class":{"$ref":"#/components/schemas/ArtifactAccess"},"byte_count":{"$ref":"#/components/schemas/DbCounter"},"created_at":{"format":"date-time","type":"string"},"created_by":{"$ref":"#/components/schemas/ArtifactProducer"},"id":{"$ref":"#/components/schemas/Id"},"kind":{"type":"string"},"media_type":{"type":"string"},"origin":{"$ref":"#/components/schemas/DataOrigin"},"producer_attempt_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"producer_run_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"project_id":{"$ref":"#/components/schemas/Id"},"schema_name":{"type":"string"},"schema_version":{"type":"string"}},"required":["id","project_id","kind","media_type","schema_name","schema_version","byte_count","access_class","origin","created_by","created_at"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};
const schema34 = {"enum":["OPERATOR","RESEARCH","EVALUATOR_ONLY","DELIVERY"],"type":"string"};
const schema35 = {"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"};
const schema36 = {"enum":["OPERATOR","RUNTIME","AGENT","IMPORT"],"type":"string"};
const schema37 = {"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"};
const schema38 = {"enum":["REAL","SYNTHETIC","FIXTURE","LEGACY_UNKNOWN"],"type":"string"};
const schema43 = {"enum":[1],"maximum":1,"minimum":1,"type":"integer"};
const func1 = Object.prototype.hasOwnProperty;
const func2 = require("ajv/dist/runtime/ucs2length").default;
const pattern4 = new RegExp("^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])", "u");
const pattern5 = new RegExp("^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$", "u");
const formats0 = require("ajv-formats/dist/formats").fullFormats["date-time"];
const formats2 = /^(?:urn:uuid:)?[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/i;

function validate22(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate22.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate22.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate22.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.kind === undefined) && (missing1 = "kind"))) || ((data1.media_type === undefined) && (missing1 = "media_type"))) || ((data1.schema_name === undefined) && (missing1 = "schema_name"))) || ((data1.schema_version === undefined) && (missing1 = "schema_version"))) || ((data1.byte_count === undefined) && (missing1 = "byte_count"))) || ((data1.access_class === undefined) && (missing1 = "access_class"))) || ((data1.origin === undefined) && (missing1 = "origin"))) || ((data1.created_by === undefined) && (missing1 = "created_by"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema33.properties.items.items.properties, key1))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.access_class !== undefined){
let data2 = data1.access_class;
const _errs7 = errors;
if(typeof data2 !== "string"){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/access_class",schemaPath:"#/components/schemas/ArtifactAccess/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data2 === "OPERATOR") || (data2 === "RESEARCH")) || (data2 === "EVALUATOR_ONLY")) || (data2 === "DELIVERY"))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/access_class",schemaPath:"#/components/schemas/ArtifactAccess/enum",keyword:"enum",params:{allowedValues: schema34.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.byte_count !== undefined){
let data3 = data1.byte_count;
const _errs10 = errors;
const _errs11 = errors;
if(errors === _errs11){
if(typeof data3 === "string"){
if(func2(data3) > 19){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/byte_count",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data3) < 1){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/byte_count",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data3)){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/byte_count",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/byte_count",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs10 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.created_at !== undefined){
let data4 = data1.created_at;
const _errs13 = errors;
if(errors === _errs13){
if(errors === _errs13){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs13 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.created_by !== undefined){
let data5 = data1.created_by;
const _errs15 = errors;
if(typeof data5 !== "string"){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/created_by",schemaPath:"#/components/schemas/ArtifactProducer/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data5 === "OPERATOR") || (data5 === "RUNTIME")) || (data5 === "AGENT")) || (data5 === "IMPORT"))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/created_by",schemaPath:"#/components/schemas/ArtifactProducer/enum",keyword:"enum",params:{allowedValues: schema36.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs15 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data6 = data1.id;
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs18 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.kind !== undefined){
const _errs21 = errors;
if(typeof data1.kind !== "string"){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/kind",schemaPath:"#/properties/items/items/properties/kind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs21 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.media_type !== undefined){
const _errs23 = errors;
if(typeof data1.media_type !== "string"){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/media_type",schemaPath:"#/properties/items/items/properties/media_type/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs23 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.origin !== undefined){
let data9 = data1.origin;
const _errs25 = errors;
if(typeof data9 !== "string"){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/origin",schemaPath:"#/components/schemas/DataOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data9 === "REAL") || (data9 === "SYNTHETIC")) || (data9 === "FIXTURE")) || (data9 === "LEGACY_UNKNOWN"))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/origin",schemaPath:"#/components/schemas/DataOrigin/enum",keyword:"enum",params:{allowedValues: schema38.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs25 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.producer_attempt_id !== undefined){
let data10 = data1.producer_attempt_id;
const _errs28 = errors;
const _errs29 = errors;
let valid8 = false;
let passing0 = null;
const _errs30 = errors;
if(data10 !== null){
const err0 = {instancePath:instancePath+"/items/" + i0+"/producer_attempt_id",schemaPath:"#/properties/items/items/properties/producer_attempt_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs30 === errors;
if(_valid0){
valid8 = true;
passing0 = 0;
}
const _errs32 = errors;
const _errs33 = errors;
if(errors === _errs33){
if(errors === _errs33){
if(typeof data10 === "string"){
if(!pattern5.test(data10)){
const err1 = {instancePath:instancePath+"/items/" + i0+"/producer_attempt_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data10))){
const err2 = {instancePath:instancePath+"/items/" + i0+"/producer_attempt_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/items/" + i0+"/producer_attempt_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs32 === errors;
if(_valid0 && valid8){
valid8 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid8 = true;
passing0 = 1;
}
}
if(!valid8){
const err4 = {instancePath:instancePath+"/items/" + i0+"/producer_attempt_id",schemaPath:"#/properties/items/items/properties/producer_attempt_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate22.errors = vErrors;
return false;
}
else {
errors = _errs29;
if(vErrors !== null){
if(_errs29){
vErrors.length = _errs29;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs28 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.producer_run_id !== undefined){
let data11 = data1.producer_run_id;
const _errs35 = errors;
const _errs36 = errors;
let valid10 = false;
let passing1 = null;
const _errs37 = errors;
if(data11 !== null){
const err5 = {instancePath:instancePath+"/items/" + i0+"/producer_run_id",schemaPath:"#/properties/items/items/properties/producer_run_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs37 === errors;
if(_valid1){
valid10 = true;
passing1 = 0;
}
const _errs39 = errors;
const _errs40 = errors;
if(errors === _errs40){
if(errors === _errs40){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err6 = {instancePath:instancePath+"/items/" + i0+"/producer_run_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err7 = {instancePath:instancePath+"/items/" + i0+"/producer_run_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/items/" + i0+"/producer_run_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs39 === errors;
if(_valid1 && valid10){
valid10 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid10 = true;
passing1 = 1;
}
}
if(!valid10){
const err9 = {instancePath:instancePath+"/items/" + i0+"/producer_run_id",schemaPath:"#/properties/items/items/properties/producer_run_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate22.errors = vErrors;
return false;
}
else {
errors = _errs36;
if(vErrors !== null){
if(_errs36){
vErrors.length = _errs36;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs35 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.project_id !== undefined){
let data12 = data1.project_id;
const _errs42 = errors;
const _errs43 = errors;
if(errors === _errs43){
if(errors === _errs43){
if(typeof data12 === "string"){
if(!pattern5.test(data12)){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data12))){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs42 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.schema_name !== undefined){
const _errs45 = errors;
if(typeof data1.schema_name !== "string"){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/schema_name",schemaPath:"#/properties/items/items/properties/schema_name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs45 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.schema_version !== undefined){
const _errs47 = errors;
if(typeof data1.schema_version !== "string"){
validate22.errors = [{instancePath:instancePath+"/items/" + i0+"/schema_version",schemaPath:"#/properties/items/items/properties/schema_version/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs47 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate22.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate22.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data15 = data.next_cursor;
const _errs49 = errors;
const _errs50 = errors;
let valid13 = false;
let passing2 = null;
const _errs51 = errors;
if(data15 !== null){
const err10 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
var _valid2 = _errs51 === errors;
if(_valid2){
valid13 = true;
passing2 = 0;
}
const _errs53 = errors;
const _errs54 = errors;
if(errors === _errs54){
if(errors === _errs54){
if(typeof data15 === "string"){
if(!pattern5.test(data15)){
const err11 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
if(!(formats2.test(data15))){
const err12 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
else {
const err13 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
}
var _valid2 = _errs53 === errors;
if(_valid2 && valid13){
valid13 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid13 = true;
passing2 = 1;
}
}
if(!valid13){
const err14 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
validate22.errors = vErrors;
return false;
}
else {
errors = _errs50;
if(vErrors !== null){
if(_errs50){
vErrors.length = _errs50;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs49 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data16 = data.schema_version;
const _errs56 = errors;
const _errs57 = errors;
if(!((typeof data16 == "number") && (!(data16 % 1) && !isNaN(data16)))){
validate22.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data16 === 1)){
validate22.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs57){
if(typeof data16 == "number"){
if(data16 > 1 || isNaN(data16)){
validate22.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data16 < 1 || isNaN(data16)){
validate22.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs56 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate22.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate22.errors = vErrors;
return errors === 0;
}
validate22.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate20(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate20.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate22(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate22.errors : vErrors.concat(validate22.errors);
errors = vErrors.length;
}
validate20.errors = vErrors;
return errors === 0;
}
validate20.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response1 = validate24;
const schema44 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1artifacts/post/responses/201/content/application~1json/schema"};
const schema45 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"access_class":{"$ref":"#/components/schemas/ArtifactAccess"},"byte_count":{"$ref":"#/components/schemas/DbCounter"},"created_at":{"format":"date-time","type":"string"},"created_by":{"$ref":"#/components/schemas/ArtifactProducer"},"id":{"$ref":"#/components/schemas/Id"},"kind":{"type":"string"},"media_type":{"type":"string"},"origin":{"$ref":"#/components/schemas/DataOrigin"},"producer_attempt_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"producer_run_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"project_id":{"$ref":"#/components/schemas/Id"},"schema_name":{"type":"string"},"schema_version":{"type":"string"}},"required":["id","project_id","kind","media_type","schema_name","schema_version","byte_count","access_class","origin","created_by","created_at"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};

function validate25(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate25.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate25.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate25.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate25.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.kind === undefined) && (missing1 = "kind"))) || ((data1.media_type === undefined) && (missing1 = "media_type"))) || ((data1.schema_name === undefined) && (missing1 = "schema_name"))) || ((data1.schema_version === undefined) && (missing1 = "schema_version"))) || ((data1.byte_count === undefined) && (missing1 = "byte_count"))) || ((data1.access_class === undefined) && (missing1 = "access_class"))) || ((data1.origin === undefined) && (missing1 = "origin"))) || ((data1.created_by === undefined) && (missing1 = "created_by"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))){
validate25.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema45.properties.resource.properties, key1))){
validate25.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.access_class !== undefined){
let data2 = data1.access_class;
const _errs7 = errors;
if(typeof data2 !== "string"){
validate25.errors = [{instancePath:instancePath+"/resource/access_class",schemaPath:"#/components/schemas/ArtifactAccess/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data2 === "OPERATOR") || (data2 === "RESEARCH")) || (data2 === "EVALUATOR_ONLY")) || (data2 === "DELIVERY"))){
validate25.errors = [{instancePath:instancePath+"/resource/access_class",schemaPath:"#/components/schemas/ArtifactAccess/enum",keyword:"enum",params:{allowedValues: schema34.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.byte_count !== undefined){
let data3 = data1.byte_count;
const _errs10 = errors;
const _errs11 = errors;
if(errors === _errs11){
if(typeof data3 === "string"){
if(func2(data3) > 19){
validate25.errors = [{instancePath:instancePath+"/resource/byte_count",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data3) < 1){
validate25.errors = [{instancePath:instancePath+"/resource/byte_count",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data3)){
validate25.errors = [{instancePath:instancePath+"/resource/byte_count",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate25.errors = [{instancePath:instancePath+"/resource/byte_count",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs10 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_at !== undefined){
let data4 = data1.created_at;
const _errs13 = errors;
if(errors === _errs13){
if(errors === _errs13){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate25.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate25.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs13 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_by !== undefined){
let data5 = data1.created_by;
const _errs15 = errors;
if(typeof data5 !== "string"){
validate25.errors = [{instancePath:instancePath+"/resource/created_by",schemaPath:"#/components/schemas/ArtifactProducer/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data5 === "OPERATOR") || (data5 === "RUNTIME")) || (data5 === "AGENT")) || (data5 === "IMPORT"))){
validate25.errors = [{instancePath:instancePath+"/resource/created_by",schemaPath:"#/components/schemas/ArtifactProducer/enum",keyword:"enum",params:{allowedValues: schema36.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs15 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data6 = data1.id;
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate25.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate25.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate25.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs18 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.kind !== undefined){
const _errs21 = errors;
if(typeof data1.kind !== "string"){
validate25.errors = [{instancePath:instancePath+"/resource/kind",schemaPath:"#/properties/resource/properties/kind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs21 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.media_type !== undefined){
const _errs23 = errors;
if(typeof data1.media_type !== "string"){
validate25.errors = [{instancePath:instancePath+"/resource/media_type",schemaPath:"#/properties/resource/properties/media_type/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs23 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.origin !== undefined){
let data9 = data1.origin;
const _errs25 = errors;
if(typeof data9 !== "string"){
validate25.errors = [{instancePath:instancePath+"/resource/origin",schemaPath:"#/components/schemas/DataOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data9 === "REAL") || (data9 === "SYNTHETIC")) || (data9 === "FIXTURE")) || (data9 === "LEGACY_UNKNOWN"))){
validate25.errors = [{instancePath:instancePath+"/resource/origin",schemaPath:"#/components/schemas/DataOrigin/enum",keyword:"enum",params:{allowedValues: schema38.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs25 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.producer_attempt_id !== undefined){
let data10 = data1.producer_attempt_id;
const _errs28 = errors;
const _errs29 = errors;
let valid7 = false;
let passing0 = null;
const _errs30 = errors;
if(data10 !== null){
const err0 = {instancePath:instancePath+"/resource/producer_attempt_id",schemaPath:"#/properties/resource/properties/producer_attempt_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs30 === errors;
if(_valid0){
valid7 = true;
passing0 = 0;
}
const _errs32 = errors;
const _errs33 = errors;
if(errors === _errs33){
if(errors === _errs33){
if(typeof data10 === "string"){
if(!pattern5.test(data10)){
const err1 = {instancePath:instancePath+"/resource/producer_attempt_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data10))){
const err2 = {instancePath:instancePath+"/resource/producer_attempt_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/producer_attempt_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs32 === errors;
if(_valid0 && valid7){
valid7 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid7 = true;
passing0 = 1;
}
}
if(!valid7){
const err4 = {instancePath:instancePath+"/resource/producer_attempt_id",schemaPath:"#/properties/resource/properties/producer_attempt_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate25.errors = vErrors;
return false;
}
else {
errors = _errs29;
if(vErrors !== null){
if(_errs29){
vErrors.length = _errs29;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs28 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.producer_run_id !== undefined){
let data11 = data1.producer_run_id;
const _errs35 = errors;
const _errs36 = errors;
let valid9 = false;
let passing1 = null;
const _errs37 = errors;
if(data11 !== null){
const err5 = {instancePath:instancePath+"/resource/producer_run_id",schemaPath:"#/properties/resource/properties/producer_run_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs37 === errors;
if(_valid1){
valid9 = true;
passing1 = 0;
}
const _errs39 = errors;
const _errs40 = errors;
if(errors === _errs40){
if(errors === _errs40){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err6 = {instancePath:instancePath+"/resource/producer_run_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err7 = {instancePath:instancePath+"/resource/producer_run_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/resource/producer_run_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs39 === errors;
if(_valid1 && valid9){
valid9 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid9 = true;
passing1 = 1;
}
}
if(!valid9){
const err9 = {instancePath:instancePath+"/resource/producer_run_id",schemaPath:"#/properties/resource/properties/producer_run_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate25.errors = vErrors;
return false;
}
else {
errors = _errs36;
if(vErrors !== null){
if(_errs36){
vErrors.length = _errs36;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs35 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.project_id !== undefined){
let data12 = data1.project_id;
const _errs42 = errors;
const _errs43 = errors;
if(errors === _errs43){
if(errors === _errs43){
if(typeof data12 === "string"){
if(!pattern5.test(data12)){
validate25.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data12))){
validate25.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate25.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs42 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.schema_name !== undefined){
const _errs45 = errors;
if(typeof data1.schema_name !== "string"){
validate25.errors = [{instancePath:instancePath+"/resource/schema_name",schemaPath:"#/properties/resource/properties/schema_name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs45 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.schema_version !== undefined){
const _errs47 = errors;
if(typeof data1.schema_version !== "string"){
validate25.errors = [{instancePath:instancePath+"/resource/schema_version",schemaPath:"#/properties/resource/properties/schema_version/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs47 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate25.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data15 = data.schema_version;
const _errs49 = errors;
const _errs50 = errors;
if(!((typeof data15 == "number") && (!(data15 % 1) && !isNaN(data15)))){
validate25.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data15 === 1)){
validate25.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs50){
if(typeof data15 == "number"){
if(data15 > 1 || isNaN(data15)){
validate25.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data15 < 1 || isNaN(data15)){
validate25.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs49 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate25.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate25.errors = vErrors;
return errors === 0;
}
validate25.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate24(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate24.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate25(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate25.errors : vErrors.concat(validate25.errors);
errors = vErrors.length;
}
validate24.errors = vErrors;
return errors === 0;
}
validate24.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response2 = validate27;
const schema55 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1artifacts~1{id}/get/responses/200/content/application~1json/schema"};
const schema56 = {"additionalProperties":false,"properties":{"access_class":{"$ref":"#/components/schemas/ArtifactAccess"},"byte_count":{"$ref":"#/components/schemas/DbCounter"},"created_at":{"format":"date-time","type":"string"},"created_by":{"$ref":"#/components/schemas/ArtifactProducer"},"id":{"$ref":"#/components/schemas/Id"},"kind":{"type":"string"},"media_type":{"type":"string"},"origin":{"$ref":"#/components/schemas/DataOrigin"},"producer_attempt_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"producer_run_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"project_id":{"$ref":"#/components/schemas/Id"},"schema_name":{"type":"string"},"schema_version":{"type":"string"}},"required":["id","project_id","kind","media_type","schema_name","schema_version","byte_count","access_class","origin","created_by","created_at"],"type":"object"};

function validate28(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate28.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((((((data.id === undefined) && (missing0 = "id")) || ((data.project_id === undefined) && (missing0 = "project_id"))) || ((data.kind === undefined) && (missing0 = "kind"))) || ((data.media_type === undefined) && (missing0 = "media_type"))) || ((data.schema_name === undefined) && (missing0 = "schema_name"))) || ((data.schema_version === undefined) && (missing0 = "schema_version"))) || ((data.byte_count === undefined) && (missing0 = "byte_count"))) || ((data.access_class === undefined) && (missing0 = "access_class"))) || ((data.origin === undefined) && (missing0 = "origin"))) || ((data.created_by === undefined) && (missing0 = "created_by"))) || ((data.created_at === undefined) && (missing0 = "created_at"))){
validate28.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema56.properties, key0))){
validate28.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.access_class !== undefined){
let data0 = data.access_class;
const _errs2 = errors;
if(typeof data0 !== "string"){
validate28.errors = [{instancePath:instancePath+"/access_class",schemaPath:"#/components/schemas/ArtifactAccess/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data0 === "OPERATOR") || (data0 === "RESEARCH")) || (data0 === "EVALUATOR_ONLY")) || (data0 === "DELIVERY"))){
validate28.errors = [{instancePath:instancePath+"/access_class",schemaPath:"#/components/schemas/ArtifactAccess/enum",keyword:"enum",params:{allowedValues: schema34.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.byte_count !== undefined){
let data1 = data.byte_count;
const _errs5 = errors;
const _errs6 = errors;
if(errors === _errs6){
if(typeof data1 === "string"){
if(func2(data1) > 19){
validate28.errors = [{instancePath:instancePath+"/byte_count",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data1) < 1){
validate28.errors = [{instancePath:instancePath+"/byte_count",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data1)){
validate28.errors = [{instancePath:instancePath+"/byte_count",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate28.errors = [{instancePath:instancePath+"/byte_count",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_at !== undefined){
let data2 = data.created_at;
const _errs8 = errors;
if(errors === _errs8){
if(errors === _errs8){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate28.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate28.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_by !== undefined){
let data3 = data.created_by;
const _errs10 = errors;
if(typeof data3 !== "string"){
validate28.errors = [{instancePath:instancePath+"/created_by",schemaPath:"#/components/schemas/ArtifactProducer/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data3 === "OPERATOR") || (data3 === "RUNTIME")) || (data3 === "AGENT")) || (data3 === "IMPORT"))){
validate28.errors = [{instancePath:instancePath+"/created_by",schemaPath:"#/components/schemas/ArtifactProducer/enum",keyword:"enum",params:{allowedValues: schema36.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs10 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data4 = data.id;
const _errs13 = errors;
const _errs14 = errors;
if(errors === _errs14){
if(errors === _errs14){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
validate28.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data4))){
validate28.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate28.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.kind !== undefined){
const _errs16 = errors;
if(typeof data.kind !== "string"){
validate28.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/properties/kind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.media_type !== undefined){
const _errs18 = errors;
if(typeof data.media_type !== "string"){
validate28.errors = [{instancePath:instancePath+"/media_type",schemaPath:"#/properties/media_type/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs18 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.origin !== undefined){
let data7 = data.origin;
const _errs20 = errors;
if(typeof data7 !== "string"){
validate28.errors = [{instancePath:instancePath+"/origin",schemaPath:"#/components/schemas/DataOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data7 === "REAL") || (data7 === "SYNTHETIC")) || (data7 === "FIXTURE")) || (data7 === "LEGACY_UNKNOWN"))){
validate28.errors = [{instancePath:instancePath+"/origin",schemaPath:"#/components/schemas/DataOrigin/enum",keyword:"enum",params:{allowedValues: schema38.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs20 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.producer_attempt_id !== undefined){
let data8 = data.producer_attempt_id;
const _errs23 = errors;
const _errs24 = errors;
let valid6 = false;
let passing0 = null;
const _errs25 = errors;
if(data8 !== null){
const err0 = {instancePath:instancePath+"/producer_attempt_id",schemaPath:"#/properties/producer_attempt_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs25 === errors;
if(_valid0){
valid6 = true;
passing0 = 0;
}
const _errs27 = errors;
const _errs28 = errors;
if(errors === _errs28){
if(errors === _errs28){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
const err1 = {instancePath:instancePath+"/producer_attempt_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data8))){
const err2 = {instancePath:instancePath+"/producer_attempt_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/producer_attempt_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs27 === errors;
if(_valid0 && valid6){
valid6 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid6 = true;
passing0 = 1;
}
}
if(!valid6){
const err4 = {instancePath:instancePath+"/producer_attempt_id",schemaPath:"#/properties/producer_attempt_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate28.errors = vErrors;
return false;
}
else {
errors = _errs24;
if(vErrors !== null){
if(_errs24){
vErrors.length = _errs24;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs23 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.producer_run_id !== undefined){
let data9 = data.producer_run_id;
const _errs30 = errors;
const _errs31 = errors;
let valid8 = false;
let passing1 = null;
const _errs32 = errors;
if(data9 !== null){
const err5 = {instancePath:instancePath+"/producer_run_id",schemaPath:"#/properties/producer_run_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs32 === errors;
if(_valid1){
valid8 = true;
passing1 = 0;
}
const _errs34 = errors;
const _errs35 = errors;
if(errors === _errs35){
if(errors === _errs35){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
const err6 = {instancePath:instancePath+"/producer_run_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data9))){
const err7 = {instancePath:instancePath+"/producer_run_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/producer_run_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs34 === errors;
if(_valid1 && valid8){
valid8 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid8 = true;
passing1 = 1;
}
}
if(!valid8){
const err9 = {instancePath:instancePath+"/producer_run_id",schemaPath:"#/properties/producer_run_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate28.errors = vErrors;
return false;
}
else {
errors = _errs31;
if(vErrors !== null){
if(_errs31){
vErrors.length = _errs31;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs30 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.project_id !== undefined){
let data10 = data.project_id;
const _errs37 = errors;
const _errs38 = errors;
if(errors === _errs38){
if(errors === _errs38){
if(typeof data10 === "string"){
if(!pattern5.test(data10)){
validate28.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data10))){
validate28.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate28.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs37 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_name !== undefined){
const _errs40 = errors;
if(typeof data.schema_name !== "string"){
validate28.errors = [{instancePath:instancePath+"/schema_name",schemaPath:"#/properties/schema_name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs40 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
const _errs42 = errors;
if(typeof data.schema_version !== "string"){
validate28.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/properties/schema_version/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs42 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate28.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate28.errors = vErrors;
return errors === 0;
}
validate28.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate27(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate27.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate28(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate28.errors : vErrors.concat(validate28.errors);
errors = vErrors.length;
}
validate27.errors = vErrors;
return errors === 0;
}
validate27.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response3 = validate30;
const schema65 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1auth~1devices/get/responses/200/content/application~1json/schema"};
const schema66 = {"additionalProperties":false,"properties":{"items":{"items":{"$ref":"#/components/schemas/TrustedDevice"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};
const schema67 = {"additionalProperties":false,"properties":{"expires_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"label":{"type":"string"},"last_used_at":{"format":"date-time","type":["string","null"]},"revoked_at":{"format":"date-time","type":["string","null"]}},"required":["id","label","expires_at"],"type":"object"};

function validate32(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate32.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.id === undefined) && (missing0 = "id")) || ((data.label === undefined) && (missing0 = "label"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))){
validate32.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((key0 === "expires_at") || (key0 === "id")) || (key0 === "label")) || (key0 === "last_used_at")) || (key0 === "revoked_at"))){
validate32.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.expires_at !== undefined){
let data0 = data.expires_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate32.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate32.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data1 = data.id;
const _errs4 = errors;
const _errs5 = errors;
if(errors === _errs5){
if(errors === _errs5){
if(typeof data1 === "string"){
if(!pattern5.test(data1)){
validate32.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data1))){
validate32.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate32.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.label !== undefined){
const _errs7 = errors;
if(typeof data.label !== "string"){
validate32.errors = [{instancePath:instancePath+"/label",schemaPath:"#/properties/label/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.last_used_at !== undefined){
let data3 = data.last_used_at;
const _errs9 = errors;
if((typeof data3 !== "string") && (data3 !== null)){
validate32.errors = [{instancePath:instancePath+"/last_used_at",schemaPath:"#/properties/last_used_at/type",keyword:"type",params:{type: schema67.properties.last_used_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs9){
if(errors === _errs9){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate32.errors = [{instancePath:instancePath+"/last_used_at",schemaPath:"#/properties/last_used_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revoked_at !== undefined){
let data4 = data.revoked_at;
const _errs11 = errors;
if((typeof data4 !== "string") && (data4 !== null)){
validate32.errors = [{instancePath:instancePath+"/revoked_at",schemaPath:"#/properties/revoked_at/type",keyword:"type",params:{type: schema67.properties.revoked_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs11){
if(errors === _errs11){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate32.errors = [{instancePath:instancePath+"/revoked_at",schemaPath:"#/properties/revoked_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
else {
validate32.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate32.errors = vErrors;
return errors === 0;
}
validate32.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate31(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate31.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate31.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate31.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs4 = errors;
if(!(validate32(data0[i0], {instancePath:instancePath+"/items/" + i0,parentData:data0,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate32.errors : vErrors.concat(validate32.errors);
errors = vErrors.length;
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate31.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data2 = data.next_cursor;
const _errs5 = errors;
const _errs6 = errors;
let valid2 = false;
let passing0 = null;
const _errs7 = errors;
if(data2 !== null){
const err0 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs7 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data2 === "string"){
if(!pattern5.test(data2)){
const err1 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data2))){
const err2 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs9 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate31.errors = vErrors;
return false;
}
else {
errors = _errs6;
if(vErrors !== null){
if(_errs6){
vErrors.length = _errs6;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data3 = data.schema_version;
const _errs12 = errors;
const _errs13 = errors;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
validate31.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data3 === 1)){
validate31.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs13){
if(typeof data3 == "number"){
if(data3 > 1 || isNaN(data3)){
validate31.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data3 < 1 || isNaN(data3)){
validate31.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs12 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate31.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate31.errors = vErrors;
return errors === 0;
}
validate31.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate30(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate30.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate31(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate31.errors : vErrors.concat(validate31.errors);
errors = vErrors.length;
}
validate30.errors = vErrors;
return errors === 0;
}
validate30.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response4 = validate35;
const schema71 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1auth~1login/post/responses/200/content/application~1json/schema"};
const schema72 = {"additionalProperties":false,"properties":{"authenticated_at":{"format":"date-time","type":"string"},"expires_at":{"format":"date-time","type":"string"},"recent_authentication_required":{"type":"boolean"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"trusted_device_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]}},"required":["schema_version","authenticated_at","expires_at","recent_authentication_required"],"type":"object"};

function validate36(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate36.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.authenticated_at === undefined) && (missing0 = "authenticated_at"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))) || ((data.recent_authentication_required === undefined) && (missing0 = "recent_authentication_required"))){
validate36.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((key0 === "authenticated_at") || (key0 === "expires_at")) || (key0 === "recent_authentication_required")) || (key0 === "schema_version")) || (key0 === "trusted_device_id"))){
validate36.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.authenticated_at !== undefined){
let data0 = data.authenticated_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate36.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate36.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.expires_at !== undefined){
let data1 = data.expires_at;
const _errs4 = errors;
if(errors === _errs4){
if(errors === _errs4){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate36.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate36.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.recent_authentication_required !== undefined){
const _errs6 = errors;
if(typeof data.recent_authentication_required !== "boolean"){
validate36.errors = [{instancePath:instancePath+"/recent_authentication_required",schemaPath:"#/properties/recent_authentication_required/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data3 = data.schema_version;
const _errs8 = errors;
const _errs9 = errors;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
validate36.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data3 === 1)){
validate36.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs9){
if(typeof data3 == "number"){
if(data3 > 1 || isNaN(data3)){
validate36.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data3 < 1 || isNaN(data3)){
validate36.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.trusted_device_id !== undefined){
let data4 = data.trusted_device_id;
const _errs11 = errors;
const _errs12 = errors;
let valid2 = false;
let passing0 = null;
const _errs13 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs13 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err1 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err2 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs15 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate36.errors = vErrors;
return false;
}
else {
errors = _errs12;
if(vErrors !== null){
if(_errs12){
vErrors.length = _errs12;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
else {
validate36.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate36.errors = vErrors;
return errors === 0;
}
validate36.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate35(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate35.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate36(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate36.errors : vErrors.concat(validate36.errors);
errors = vErrors.length;
}
validate35.errors = vErrors;
return errors === 0;
}
validate35.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response5 = validate38;
const schema75 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1auth~1machine/get/responses/200/content/application~1json/schema"};
const schema76 = {"additionalProperties":false,"description":"Public identity of this verified machine credential, never a secret lookup.","properties":{"credential_id":{"$ref":"#/components/schemas/Id"},"downstream_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"expires_at":{"format":"date-time","type":"string"},"kind":{"$ref":"#/components/schemas/PrincipalKind"},"project_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"run_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"scope_codes":{"items":{"$ref":"#/components/schemas/MachineScope"},"type":"array"}},"required":["schema_version","credential_id","kind","scope_codes","expires_at"],"type":"object"};
const schema79 = {"enum":["CLI","DOWNSTREAM","AUTOMATION","MISSION"],"type":"string"};
const schema83 = {"enum":["RESEARCH_READ","EXPERIMENT_SUBMIT","ARTIFACT_SUBMIT","EVIDENCE_READ","RUN_READ","RUN_CANCEL","DOWNSTREAM_CLAIM","DOWNSTREAM_ACK","FORWARD_SUBMIT","DOCTOR_READ"],"type":"string"};

function validate39(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate39.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.credential_id === undefined) && (missing0 = "credential_id"))) || ((data.kind === undefined) && (missing0 = "kind"))) || ((data.scope_codes === undefined) && (missing0 = "scope_codes"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))){
validate39.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((((((key0 === "credential_id") || (key0 === "downstream_id")) || (key0 === "expires_at")) || (key0 === "kind")) || (key0 === "project_id")) || (key0 === "run_id")) || (key0 === "schema_version")) || (key0 === "scope_codes"))){
validate39.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.credential_id !== undefined){
let data0 = data.credential_id;
const _errs2 = errors;
const _errs3 = errors;
if(errors === _errs3){
if(errors === _errs3){
if(typeof data0 === "string"){
if(!pattern5.test(data0)){
validate39.errors = [{instancePath:instancePath+"/credential_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data0))){
validate39.errors = [{instancePath:instancePath+"/credential_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate39.errors = [{instancePath:instancePath+"/credential_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.downstream_id !== undefined){
let data1 = data.downstream_id;
const _errs5 = errors;
const _errs6 = errors;
let valid2 = false;
let passing0 = null;
const _errs7 = errors;
if(data1 !== null){
const err0 = {instancePath:instancePath+"/downstream_id",schemaPath:"#/properties/downstream_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs7 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data1 === "string"){
if(!pattern5.test(data1)){
const err1 = {instancePath:instancePath+"/downstream_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data1))){
const err2 = {instancePath:instancePath+"/downstream_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/downstream_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs9 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/downstream_id",schemaPath:"#/properties/downstream_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate39.errors = vErrors;
return false;
}
else {
errors = _errs6;
if(vErrors !== null){
if(_errs6){
vErrors.length = _errs6;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.expires_at !== undefined){
let data2 = data.expires_at;
const _errs12 = errors;
if(errors === _errs12){
if(errors === _errs12){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate39.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate39.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs12 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.kind !== undefined){
let data3 = data.kind;
const _errs14 = errors;
if(typeof data3 !== "string"){
validate39.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/components/schemas/PrincipalKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data3 === "CLI") || (data3 === "DOWNSTREAM")) || (data3 === "AUTOMATION")) || (data3 === "MISSION"))){
validate39.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/components/schemas/PrincipalKind/enum",keyword:"enum",params:{allowedValues: schema79.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs14 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.project_id !== undefined){
let data4 = data.project_id;
const _errs17 = errors;
const _errs18 = errors;
let valid5 = false;
let passing1 = null;
const _errs19 = errors;
if(data4 !== null){
const err5 = {instancePath:instancePath+"/project_id",schemaPath:"#/properties/project_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs19 === errors;
if(_valid1){
valid5 = true;
passing1 = 0;
}
const _errs21 = errors;
const _errs22 = errors;
if(errors === _errs22){
if(errors === _errs22){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err6 = {instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err7 = {instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs21 === errors;
if(_valid1 && valid5){
valid5 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid5 = true;
passing1 = 1;
}
}
if(!valid5){
const err9 = {instancePath:instancePath+"/project_id",schemaPath:"#/properties/project_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate39.errors = vErrors;
return false;
}
else {
errors = _errs18;
if(vErrors !== null){
if(_errs18){
vErrors.length = _errs18;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs17 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_id !== undefined){
let data5 = data.run_id;
const _errs24 = errors;
const _errs25 = errors;
let valid7 = false;
let passing2 = null;
const _errs26 = errors;
if(data5 !== null){
const err10 = {instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
var _valid2 = _errs26 === errors;
if(_valid2){
valid7 = true;
passing2 = 0;
}
const _errs28 = errors;
const _errs29 = errors;
if(errors === _errs29){
if(errors === _errs29){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
const err11 = {instancePath:instancePath+"/run_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
if(!(formats2.test(data5))){
const err12 = {instancePath:instancePath+"/run_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
else {
const err13 = {instancePath:instancePath+"/run_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
}
var _valid2 = _errs28 === errors;
if(_valid2 && valid7){
valid7 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid7 = true;
passing2 = 1;
}
}
if(!valid7){
const err14 = {instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
validate39.errors = vErrors;
return false;
}
else {
errors = _errs25;
if(vErrors !== null){
if(_errs25){
vErrors.length = _errs25;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs24 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data6 = data.schema_version;
const _errs31 = errors;
const _errs32 = errors;
if(!((typeof data6 == "number") && (!(data6 % 1) && !isNaN(data6)))){
validate39.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data6 === 1)){
validate39.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs32){
if(typeof data6 == "number"){
if(data6 > 1 || isNaN(data6)){
validate39.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data6 < 1 || isNaN(data6)){
validate39.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs31 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.scope_codes !== undefined){
let data7 = data.scope_codes;
const _errs34 = errors;
if(errors === _errs34){
if(Array.isArray(data7)){
var valid10 = true;
const len0 = data7.length;
for(let i0=0; i0<len0; i0++){
let data8 = data7[i0];
const _errs36 = errors;
if(typeof data8 !== "string"){
validate39.errors = [{instancePath:instancePath+"/scope_codes/" + i0,schemaPath:"#/components/schemas/MachineScope/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((((data8 === "RESEARCH_READ") || (data8 === "EXPERIMENT_SUBMIT")) || (data8 === "ARTIFACT_SUBMIT")) || (data8 === "EVIDENCE_READ")) || (data8 === "RUN_READ")) || (data8 === "RUN_CANCEL")) || (data8 === "DOWNSTREAM_CLAIM")) || (data8 === "DOWNSTREAM_ACK")) || (data8 === "FORWARD_SUBMIT")) || (data8 === "DOCTOR_READ"))){
validate39.errors = [{instancePath:instancePath+"/scope_codes/" + i0,schemaPath:"#/components/schemas/MachineScope/enum",keyword:"enum",params:{allowedValues: schema83.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid10 = _errs36 === errors;
if(!valid10){
break;
}
}
}
else {
validate39.errors = [{instancePath:instancePath+"/scope_codes",schemaPath:"#/properties/scope_codes/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs34 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
else {
validate39.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate39.errors = vErrors;
return errors === 0;
}
validate39.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate38(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate38.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate39(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate39.errors : vErrors.concat(validate39.errors);
errors = vErrors.length;
}
validate38.errors = vErrors;
return errors === 0;
}
validate38.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response6 = validate41;
const schema84 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1auth~1operator-command-grants/post/responses/201/content/application~1json/schema"};
const schema85 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"auth_epoch":{"$ref":"#/components/schemas/Revision"},"authenticated_at":{"format":"date-time","type":"string"},"credential_id":{"$ref":"#/components/schemas/Id"},"expires_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"operation":{"$ref":"#/components/schemas/OperatorOperation"},"target_id":{"$ref":"#/components/schemas/Id"}},"required":["id","credential_id","operation","target_id","auth_epoch","authenticated_at","expires_at"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};
const schema86 = {"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"};
const schema89 = {"enum":["BRIEF_CREATE","BRIEF_UPDATE","PROJECT_CREATE","PROJECT_UPDATE","PRINCIPAL_CREATE","PRINCIPAL_UPDATE","CREDENTIAL_ISSUE","CREDENTIAL_REVOKE","INPUT_SET_CREATE","EVALUATION_POLICY_CREATE"],"type":"string"};
const pattern27 = new RegExp("^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])", "u");

function validate42(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate42.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate42.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate42.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate42.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.credential_id === undefined) && (missing1 = "credential_id"))) || ((data1.operation === undefined) && (missing1 = "operation"))) || ((data1.target_id === undefined) && (missing1 = "target_id"))) || ((data1.auth_epoch === undefined) && (missing1 = "auth_epoch"))) || ((data1.authenticated_at === undefined) && (missing1 = "authenticated_at"))) || ((data1.expires_at === undefined) && (missing1 = "expires_at"))){
validate42.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(((((((key1 === "auth_epoch") || (key1 === "authenticated_at")) || (key1 === "credential_id")) || (key1 === "expires_at")) || (key1 === "id")) || (key1 === "operation")) || (key1 === "target_id"))){
validate42.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.auth_epoch !== undefined){
let data2 = data1.auth_epoch;
const _errs7 = errors;
const _errs8 = errors;
if(errors === _errs8){
if(typeof data2 === "string"){
if(func2(data2) > 19){
validate42.errors = [{instancePath:instancePath+"/resource/auth_epoch",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data2) < 1){
validate42.errors = [{instancePath:instancePath+"/resource/auth_epoch",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data2)){
validate42.errors = [{instancePath:instancePath+"/resource/auth_epoch",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate42.errors = [{instancePath:instancePath+"/resource/auth_epoch",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.authenticated_at !== undefined){
let data3 = data1.authenticated_at;
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate42.errors = [{instancePath:instancePath+"/resource/authenticated_at",schemaPath:"#/properties/resource/properties/authenticated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate42.errors = [{instancePath:instancePath+"/resource/authenticated_at",schemaPath:"#/properties/resource/properties/authenticated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs10 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.credential_id !== undefined){
let data4 = data1.credential_id;
const _errs12 = errors;
const _errs13 = errors;
if(errors === _errs13){
if(errors === _errs13){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
validate42.errors = [{instancePath:instancePath+"/resource/credential_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data4))){
validate42.errors = [{instancePath:instancePath+"/resource/credential_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate42.errors = [{instancePath:instancePath+"/resource/credential_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs12 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.expires_at !== undefined){
let data5 = data1.expires_at;
const _errs15 = errors;
if(errors === _errs15){
if(errors === _errs15){
if(typeof data5 === "string"){
if(!(formats0.validate(data5))){
validate42.errors = [{instancePath:instancePath+"/resource/expires_at",schemaPath:"#/properties/resource/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate42.errors = [{instancePath:instancePath+"/resource/expires_at",schemaPath:"#/properties/resource/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs15 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data6 = data1.id;
const _errs17 = errors;
const _errs18 = errors;
if(errors === _errs18){
if(errors === _errs18){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate42.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate42.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate42.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs17 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.operation !== undefined){
let data7 = data1.operation;
const _errs20 = errors;
if(typeof data7 !== "string"){
validate42.errors = [{instancePath:instancePath+"/resource/operation",schemaPath:"#/components/schemas/OperatorOperation/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((((data7 === "BRIEF_CREATE") || (data7 === "BRIEF_UPDATE")) || (data7 === "PROJECT_CREATE")) || (data7 === "PROJECT_UPDATE")) || (data7 === "PRINCIPAL_CREATE")) || (data7 === "PRINCIPAL_UPDATE")) || (data7 === "CREDENTIAL_ISSUE")) || (data7 === "CREDENTIAL_REVOKE")) || (data7 === "INPUT_SET_CREATE")) || (data7 === "EVALUATION_POLICY_CREATE"))){
validate42.errors = [{instancePath:instancePath+"/resource/operation",schemaPath:"#/components/schemas/OperatorOperation/enum",keyword:"enum",params:{allowedValues: schema89.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs20 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.target_id !== undefined){
let data8 = data1.target_id;
const _errs23 = errors;
const _errs24 = errors;
if(errors === _errs24){
if(errors === _errs24){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate42.errors = [{instancePath:instancePath+"/resource/target_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate42.errors = [{instancePath:instancePath+"/resource/target_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate42.errors = [{instancePath:instancePath+"/resource/target_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs23 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
else {
validate42.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data9 = data.schema_version;
const _errs26 = errors;
const _errs27 = errors;
if(!((typeof data9 == "number") && (!(data9 % 1) && !isNaN(data9)))){
validate42.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data9 === 1)){
validate42.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs27){
if(typeof data9 == "number"){
if(data9 > 1 || isNaN(data9)){
validate42.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data9 < 1 || isNaN(data9)){
validate42.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs26 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate42.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate42.errors = vErrors;
return errors === 0;
}
validate42.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate41(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate41.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate42(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate42.errors : vErrors.concat(validate42.errors);
errors = vErrors.length;
}
validate41.errors = vErrors;
return errors === 0;
}
validate41.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response7 = validate44;
const schema92 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1auth~1session/get/responses/200/content/application~1json/schema"};

function validate45(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate45.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.authenticated_at === undefined) && (missing0 = "authenticated_at"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))) || ((data.recent_authentication_required === undefined) && (missing0 = "recent_authentication_required"))){
validate45.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((key0 === "authenticated_at") || (key0 === "expires_at")) || (key0 === "recent_authentication_required")) || (key0 === "schema_version")) || (key0 === "trusted_device_id"))){
validate45.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.authenticated_at !== undefined){
let data0 = data.authenticated_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate45.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate45.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.expires_at !== undefined){
let data1 = data.expires_at;
const _errs4 = errors;
if(errors === _errs4){
if(errors === _errs4){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate45.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate45.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.recent_authentication_required !== undefined){
const _errs6 = errors;
if(typeof data.recent_authentication_required !== "boolean"){
validate45.errors = [{instancePath:instancePath+"/recent_authentication_required",schemaPath:"#/properties/recent_authentication_required/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data3 = data.schema_version;
const _errs8 = errors;
const _errs9 = errors;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
validate45.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data3 === 1)){
validate45.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs9){
if(typeof data3 == "number"){
if(data3 > 1 || isNaN(data3)){
validate45.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data3 < 1 || isNaN(data3)){
validate45.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.trusted_device_id !== undefined){
let data4 = data.trusted_device_id;
const _errs11 = errors;
const _errs12 = errors;
let valid2 = false;
let passing0 = null;
const _errs13 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs13 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err1 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err2 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs15 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate45.errors = vErrors;
return false;
}
else {
errors = _errs12;
if(vErrors !== null){
if(_errs12){
vErrors.length = _errs12;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
else {
validate45.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate45.errors = vErrors;
return errors === 0;
}
validate45.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate44(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate44.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate45(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate45.errors : vErrors.concat(validate45.errors);
errors = vErrors.length;
}
validate44.errors = vErrors;
return errors === 0;
}
validate44.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response8 = validate47;
const schema96 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1auth~1verify/post/responses/200/content/application~1json/schema"};

function validate48(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate48.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.authenticated_at === undefined) && (missing0 = "authenticated_at"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))) || ((data.recent_authentication_required === undefined) && (missing0 = "recent_authentication_required"))){
validate48.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((key0 === "authenticated_at") || (key0 === "expires_at")) || (key0 === "recent_authentication_required")) || (key0 === "schema_version")) || (key0 === "trusted_device_id"))){
validate48.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.authenticated_at !== undefined){
let data0 = data.authenticated_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate48.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate48.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.expires_at !== undefined){
let data1 = data.expires_at;
const _errs4 = errors;
if(errors === _errs4){
if(errors === _errs4){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate48.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate48.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.recent_authentication_required !== undefined){
const _errs6 = errors;
if(typeof data.recent_authentication_required !== "boolean"){
validate48.errors = [{instancePath:instancePath+"/recent_authentication_required",schemaPath:"#/properties/recent_authentication_required/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data3 = data.schema_version;
const _errs8 = errors;
const _errs9 = errors;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
validate48.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data3 === 1)){
validate48.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs9){
if(typeof data3 == "number"){
if(data3 > 1 || isNaN(data3)){
validate48.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data3 < 1 || isNaN(data3)){
validate48.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.trusted_device_id !== undefined){
let data4 = data.trusted_device_id;
const _errs11 = errors;
const _errs12 = errors;
let valid2 = false;
let passing0 = null;
const _errs13 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs13 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err1 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err2 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs15 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate48.errors = vErrors;
return false;
}
else {
errors = _errs12;
if(vErrors !== null){
if(_errs12){
vErrors.length = _errs12;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
else {
validate48.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate48.errors = vErrors;
return errors === 0;
}
validate48.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate47(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate47.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate48(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate48.errors : vErrors.concat(validate48.errors);
errors = vErrors.length;
}
validate47.errors = vErrors;
return errors === 0;
}
validate47.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response9 = validate50;
const schema100 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1bootstrap~1confirm/post/responses/200/content/application~1json/schema"};

function validate51(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate51.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.authenticated_at === undefined) && (missing0 = "authenticated_at"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))) || ((data.recent_authentication_required === undefined) && (missing0 = "recent_authentication_required"))){
validate51.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((key0 === "authenticated_at") || (key0 === "expires_at")) || (key0 === "recent_authentication_required")) || (key0 === "schema_version")) || (key0 === "trusted_device_id"))){
validate51.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.authenticated_at !== undefined){
let data0 = data.authenticated_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate51.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate51.errors = [{instancePath:instancePath+"/authenticated_at",schemaPath:"#/properties/authenticated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.expires_at !== undefined){
let data1 = data.expires_at;
const _errs4 = errors;
if(errors === _errs4){
if(errors === _errs4){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate51.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate51.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.recent_authentication_required !== undefined){
const _errs6 = errors;
if(typeof data.recent_authentication_required !== "boolean"){
validate51.errors = [{instancePath:instancePath+"/recent_authentication_required",schemaPath:"#/properties/recent_authentication_required/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data3 = data.schema_version;
const _errs8 = errors;
const _errs9 = errors;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
validate51.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data3 === 1)){
validate51.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs9){
if(typeof data3 == "number"){
if(data3 > 1 || isNaN(data3)){
validate51.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data3 < 1 || isNaN(data3)){
validate51.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.trusted_device_id !== undefined){
let data4 = data.trusted_device_id;
const _errs11 = errors;
const _errs12 = errors;
let valid2 = false;
let passing0 = null;
const _errs13 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs13 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err1 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err2 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs15 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/trusted_device_id",schemaPath:"#/properties/trusted_device_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate51.errors = vErrors;
return false;
}
else {
errors = _errs12;
if(vErrors !== null){
if(_errs12){
vErrors.length = _errs12;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
else {
validate51.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate51.errors = vErrors;
return errors === 0;
}
validate51.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate50(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate50.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate51(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate51.errors : vErrors.concat(validate51.errors);
errors = vErrors.length;
}
validate50.errors = vErrors;
return errors === 0;
}
validate50.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response10 = validate53;
const schema104 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1bootstrap~1start/post/responses/201/content/application~1json/schema"};
const schema105 = {"additionalProperties":false,"properties":{"enrollment_id":{"$ref":"#/components/schemas/Id"},"expires_at":{"format":"date-time","type":"string"},"provisioning_uri":{"type":"string"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","enrollment_id","expires_at","provisioning_uri"],"type":"object"};

function validate54(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate54.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.enrollment_id === undefined) && (missing0 = "enrollment_id"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))) || ((data.provisioning_uri === undefined) && (missing0 = "provisioning_uri"))){
validate54.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((key0 === "enrollment_id") || (key0 === "expires_at")) || (key0 === "provisioning_uri")) || (key0 === "schema_version"))){
validate54.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.enrollment_id !== undefined){
let data0 = data.enrollment_id;
const _errs2 = errors;
const _errs3 = errors;
if(errors === _errs3){
if(errors === _errs3){
if(typeof data0 === "string"){
if(!pattern5.test(data0)){
validate54.errors = [{instancePath:instancePath+"/enrollment_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data0))){
validate54.errors = [{instancePath:instancePath+"/enrollment_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate54.errors = [{instancePath:instancePath+"/enrollment_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.expires_at !== undefined){
let data1 = data.expires_at;
const _errs5 = errors;
if(errors === _errs5){
if(errors === _errs5){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate54.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate54.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.provisioning_uri !== undefined){
const _errs7 = errors;
if(typeof data.provisioning_uri !== "string"){
validate54.errors = [{instancePath:instancePath+"/provisioning_uri",schemaPath:"#/properties/provisioning_uri/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data3 = data.schema_version;
const _errs9 = errors;
const _errs10 = errors;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
validate54.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data3 === 1)){
validate54.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs10){
if(typeof data3 == "number"){
if(data3 > 1 || isNaN(data3)){
validate54.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data3 < 1 || isNaN(data3)){
validate54.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate54.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate54.errors = vErrors;
return errors === 0;
}
validate54.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate53(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate53.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate54(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate54.errors : vErrors.concat(validate54.errors);
errors = vErrors.length;
}
validate53.errors = vErrors;
return errors === 0;
}
validate53.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response11 = validate56;
const schema108 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1bootstrap~1status/get/responses/200/content/application~1json/schema"};
const schema109 = {"additionalProperties":false,"properties":{"initialized":{"type":"boolean"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"setup_allowed":{"type":"boolean"}},"required":["schema_version","initialized","setup_allowed"],"type":"object"};

function validate57(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate57.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.initialized === undefined) && (missing0 = "initialized"))) || ((data.setup_allowed === undefined) && (missing0 = "setup_allowed"))){
validate57.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "initialized") || (key0 === "schema_version")) || (key0 === "setup_allowed"))){
validate57.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.initialized !== undefined){
const _errs2 = errors;
if(typeof data.initialized !== "boolean"){
validate57.errors = [{instancePath:instancePath+"/initialized",schemaPath:"#/properties/initialized/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data1 = data.schema_version;
const _errs4 = errors;
const _errs5 = errors;
if(!((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1)))){
validate57.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data1 === 1)){
validate57.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs5){
if(typeof data1 == "number"){
if(data1 > 1 || isNaN(data1)){
validate57.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data1 < 1 || isNaN(data1)){
validate57.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.setup_allowed !== undefined){
const _errs7 = errors;
if(typeof data.setup_allowed !== "boolean"){
validate57.errors = [{instancePath:instancePath+"/setup_allowed",schemaPath:"#/properties/setup_allowed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate57.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate57.errors = vErrors;
return errors === 0;
}
validate57.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate56(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate56.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate57(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate57.errors : vErrors.concat(validate57.errors);
errors = vErrors.length;
}
validate56.errors = vErrors;
return errors === 0;
}
validate56.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response12 = validate59;
const schema111 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1briefs~1{id}/get/responses/200/content/application~1json/schema"};
const schema112 = {"additionalProperties":false,"properties":{"bindings":{"items":{"$ref":"#/components/schemas/BriefBindingV1"},"maxItems":64,"minItems":1,"type":"array"},"content":{"$ref":"#/components/schemas/BriefContentV1"},"created_at":{"format":"date-time","type":"string"},"frozen_at":{"format":"date-time","type":["string","null"]},"id":{"$ref":"#/components/schemas/Id"},"project_id":{"$ref":"#/components/schemas/Id"},"revision":{"$ref":"#/components/schemas/Revision"},"state":{"$ref":"#/components/schemas/BriefState"},"supersedes_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"updated_at":{"format":"date-time","type":"string"},"version":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"}},"required":["id","project_id","version","revision","state","content","bindings","created_at","updated_at"],"type":"object"};
const schema133 = {"enum":["DRAFT","FROZEN"],"type":"string"};
const schema113 = {"additionalProperties":false,"properties":{"access_policy":{"$ref":"#/components/schemas/DataAccess"},"dataset_revision_id":{"$ref":"#/components/schemas/Id"},"role":{"$ref":"#/components/schemas/DataPartition"}},"required":["dataset_revision_id","role","access_policy"],"type":"object"};
const schema114 = {"enum":["METADATA_ONLY","RESEARCH_READ","EVALUATOR_ONLY"],"type":"string"};
const schema116 = {"enum":["DISCOVERY","VALIDATION","SEALED","FORWARD"],"type":"string"};

function validate61(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate61.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.dataset_revision_id === undefined) && (missing0 = "dataset_revision_id")) || ((data.role === undefined) && (missing0 = "role"))) || ((data.access_policy === undefined) && (missing0 = "access_policy"))){
validate61.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "access_policy") || (key0 === "dataset_revision_id")) || (key0 === "role"))){
validate61.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.access_policy !== undefined){
let data0 = data.access_policy;
const _errs2 = errors;
if(typeof data0 !== "string"){
validate61.errors = [{instancePath:instancePath+"/access_policy",schemaPath:"#/components/schemas/DataAccess/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(((data0 === "METADATA_ONLY") || (data0 === "RESEARCH_READ")) || (data0 === "EVALUATOR_ONLY"))){
validate61.errors = [{instancePath:instancePath+"/access_policy",schemaPath:"#/components/schemas/DataAccess/enum",keyword:"enum",params:{allowedValues: schema114.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.dataset_revision_id !== undefined){
let data1 = data.dataset_revision_id;
const _errs5 = errors;
const _errs6 = errors;
if(errors === _errs6){
if(errors === _errs6){
if(typeof data1 === "string"){
if(!pattern5.test(data1)){
validate61.errors = [{instancePath:instancePath+"/dataset_revision_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data1))){
validate61.errors = [{instancePath:instancePath+"/dataset_revision_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate61.errors = [{instancePath:instancePath+"/dataset_revision_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.role !== undefined){
let data2 = data.role;
const _errs8 = errors;
if(typeof data2 !== "string"){
validate61.errors = [{instancePath:instancePath+"/role",schemaPath:"#/components/schemas/DataPartition/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data2 === "DISCOVERY") || (data2 === "VALIDATION")) || (data2 === "SEALED")) || (data2 === "FORWARD"))){
validate61.errors = [{instancePath:instancePath+"/role",schemaPath:"#/components/schemas/DataPartition/enum",keyword:"enum",params:{allowedValues: schema116.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate61.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate61.errors = vErrors;
return errors === 0;
}
validate61.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

const schema117 = {"oneOf":[{"additionalProperties":false,"properties":{"base_currency":{"maxLength":3,"minLength":3,"pattern":"^[A-Z]{3}$","type":"string"},"benchmark_ref":{"oneOf":[{"type":"null"},{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"}]},"budget":{"additionalProperties":false,"properties":{"cost_currency":{"type":["string","null"]},"cost_enforcement":{"$ref":"#/components/schemas/CostEnforcement"},"max_cost_decimal":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/DecimalValue"}]},"max_cpu_seconds":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"max_cycles_per_day":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_experiments":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"max_memory_mib":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"max_output_bytes":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"max_parallel_runs":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_repair_turns":{"format":"int32","maximum":65535,"minimum":0,"type":"integer"},"max_tokens":{"oneOf":[{"type":"null"},{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"}]},"max_turns_per_mission":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_wall_seconds":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"min_cycle_interval_seconds":{"format":"int64","maximum":4294967295,"minimum":0,"type":"integer"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","max_experiments","max_parallel_runs","max_turns_per_mission","max_repair_turns","max_wall_seconds","max_cpu_seconds","max_memory_mib","max_output_bytes","max_cycles_per_day","min_cycle_interval_seconds","cost_enforcement"],"type":"object"},"economic_rationale":{"maxLength":8000,"minLength":1,"type":"string"},"evaluation_policy_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"},"execution_assumptions_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"},"horizon_kind":{"enum":["FIXED_BARS"],"type":"string"},"horizon_value":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"hypothesis":{"maxLength":8000,"minLength":1,"type":"string"},"stop_rule":{"additionalProperties":false,"properties":{"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"stop_on_budget":{"type":"boolean"},"stop_on_invalid_data":{"type":"boolean"},"stop_on_no_improvement_trials":{"format":"int32","maximum":65535,"minimum":1,"type":["integer","null"]},"stop_on_qualified_count":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"}},"required":["schema_version","stop_on_qualified_count","stop_on_budget","stop_on_invalid_data"],"type":"object"},"target_kind":{"enum":["SCORE","EXPECTED_RETURN"],"type":"string"},"universe_version_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"}},"required":["hypothesis","economic_rationale","universe_version_id","target_kind","horizon_kind","base_currency","evaluation_policy_id","execution_assumptions_id","budget","stop_rule","horizon_value"],"type":"object"},{"additionalProperties":false,"properties":{"base_currency":{"maxLength":3,"minLength":3,"pattern":"^[A-Z]{3}$","type":"string"},"benchmark_ref":{"oneOf":[{"type":"null"},{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"}]},"budget":{"additionalProperties":false,"properties":{"cost_currency":{"type":["string","null"]},"cost_enforcement":{"$ref":"#/components/schemas/CostEnforcement"},"max_cost_decimal":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/DecimalValue"}]},"max_cpu_seconds":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"max_cycles_per_day":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_experiments":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"max_memory_mib":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"max_output_bytes":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"max_parallel_runs":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_repair_turns":{"format":"int32","maximum":65535,"minimum":0,"type":"integer"},"max_tokens":{"oneOf":[{"type":"null"},{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"}]},"max_turns_per_mission":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_wall_seconds":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"min_cycle_interval_seconds":{"format":"int64","maximum":4294967295,"minimum":0,"type":"integer"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","max_experiments","max_parallel_runs","max_turns_per_mission","max_repair_turns","max_wall_seconds","max_cpu_seconds","max_memory_mib","max_output_bytes","max_cycles_per_day","min_cycle_interval_seconds","cost_enforcement"],"type":"object"},"economic_rationale":{"maxLength":8000,"minLength":1,"type":"string"},"evaluation_policy_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"},"execution_assumptions_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"},"horizon_kind":{"enum":["FIXED_DURATION"],"type":"string"},"horizon_value":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"hypothesis":{"maxLength":8000,"minLength":1,"type":"string"},"stop_rule":{"additionalProperties":false,"properties":{"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"stop_on_budget":{"type":"boolean"},"stop_on_invalid_data":{"type":"boolean"},"stop_on_no_improvement_trials":{"format":"int32","maximum":65535,"minimum":1,"type":["integer","null"]},"stop_on_qualified_count":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"}},"required":["schema_version","stop_on_qualified_count","stop_on_budget","stop_on_invalid_data"],"type":"object"},"target_kind":{"enum":["SCORE","EXPECTED_RETURN"],"type":"string"},"universe_version_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"}},"required":["hypothesis","economic_rationale","universe_version_id","target_kind","horizon_kind","base_currency","evaluation_policy_id","execution_assumptions_id","budget","stop_rule","horizon_value"],"type":"object"},{"additionalProperties":false,"properties":{"base_currency":{"maxLength":3,"minLength":3,"pattern":"^[A-Z]{3}$","type":"string"},"benchmark_ref":{"oneOf":[{"type":"null"},{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"}]},"budget":{"additionalProperties":false,"properties":{"cost_currency":{"type":["string","null"]},"cost_enforcement":{"$ref":"#/components/schemas/CostEnforcement"},"max_cost_decimal":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/DecimalValue"}]},"max_cpu_seconds":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"max_cycles_per_day":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_experiments":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"max_memory_mib":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"max_output_bytes":{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"},"max_parallel_runs":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_repair_turns":{"format":"int32","maximum":65535,"minimum":0,"type":"integer"},"max_tokens":{"oneOf":[{"type":"null"},{"description":"Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions.","maxLength":19,"minLength":1,"pattern":"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])","type":"string"}]},"max_turns_per_mission":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"max_wall_seconds":{"format":"int64","maximum":4294967295,"minimum":1,"type":"integer"},"min_cycle_interval_seconds":{"format":"int64","maximum":4294967295,"minimum":0,"type":"integer"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","max_experiments","max_parallel_runs","max_turns_per_mission","max_repair_turns","max_wall_seconds","max_cpu_seconds","max_memory_mib","max_output_bytes","max_cycles_per_day","min_cycle_interval_seconds","cost_enforcement"],"type":"object"},"economic_rationale":{"maxLength":8000,"minLength":1,"type":"string"},"evaluation_policy_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"},"execution_assumptions_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"},"horizon_kind":{"enum":["VARIABLE_INTERVAL"],"type":"string"},"horizon_value":{"type":"null"},"hypothesis":{"maxLength":8000,"minLength":1,"type":"string"},"stop_rule":{"additionalProperties":false,"properties":{"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"stop_on_budget":{"type":"boolean"},"stop_on_invalid_data":{"type":"boolean"},"stop_on_no_improvement_trials":{"format":"int32","maximum":65535,"minimum":1,"type":["integer","null"]},"stop_on_qualified_count":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"}},"required":["schema_version","stop_on_qualified_count","stop_on_budget","stop_on_invalid_data"],"type":"object"},"target_kind":{"enum":["SCORE","EXPECTED_RETURN"],"type":"string"},"universe_version_id":{"format":"uuid","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$","type":"string"}},"required":["hypothesis","economic_rationale","universe_version_id","target_kind","horizon_kind","base_currency","evaluation_policy_id","execution_assumptions_id","budget","stop_rule"],"type":"object"}]};
const schema118 = {"enum":["UNAVAILABLE","ESTIMATED","EXACT"],"type":"string"};
const schema119 = {"description":"Plain decimal exactly representable by NUMERIC(38,18).","maxLength":64,"minLength":1,"pattern":"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])","type":"string"};
const pattern36 = new RegExp("^[A-Z]{3}$", "u");
const pattern38 = new RegExp("^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])", "u");
const formats94 = require("ajv-formats/dist/formats").fullFormats.int32;
const formats96 = require("ajv-formats/dist/formats").fullFormats.int64;

function validate63(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate63.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
const _errs0 = errors;
let valid0 = false;
let passing0 = null;
const _errs1 = errors;
if(errors === _errs1){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((((((data.hypothesis === undefined) && (missing0 = "hypothesis")) || ((data.economic_rationale === undefined) && (missing0 = "economic_rationale"))) || ((data.universe_version_id === undefined) && (missing0 = "universe_version_id"))) || ((data.target_kind === undefined) && (missing0 = "target_kind"))) || ((data.horizon_kind === undefined) && (missing0 = "horizon_kind"))) || ((data.base_currency === undefined) && (missing0 = "base_currency"))) || ((data.evaluation_policy_id === undefined) && (missing0 = "evaluation_policy_id"))) || ((data.execution_assumptions_id === undefined) && (missing0 = "execution_assumptions_id"))) || ((data.budget === undefined) && (missing0 = "budget"))) || ((data.stop_rule === undefined) && (missing0 = "stop_rule"))) || ((data.horizon_value === undefined) && (missing0 = "horizon_value"))){
const err0 = {instancePath,schemaPath:"#/oneOf/0/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
else {
const _errs3 = errors;
for(const key0 in data){
if(!(func1.call(schema117.oneOf[0].properties, key0))){
const err1 = {instancePath,schemaPath:"#/oneOf/0/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
break;
}
}
if(_errs3 === errors){
if(data.base_currency !== undefined){
let data0 = data.base_currency;
const _errs4 = errors;
if(errors === _errs4){
if(typeof data0 === "string"){
if(func2(data0) > 3){
const err2 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/0/properties/base_currency/maxLength",keyword:"maxLength",params:{limit: 3},message:"must NOT have more than 3 characters"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
else {
if(func2(data0) < 3){
const err3 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/0/properties/base_currency/minLength",keyword:"minLength",params:{limit: 3},message:"must NOT have fewer than 3 characters"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
else {
if(!pattern36.test(data0)){
const err4 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/0/properties/base_currency/pattern",keyword:"pattern",params:{pattern: "^[A-Z]{3}$"},message:"must match pattern \""+"^[A-Z]{3}$"+"\""};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
}
}
}
else {
const err5 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/0/properties/base_currency/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
}
var valid1 = _errs4 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.benchmark_ref !== undefined){
let data1 = data.benchmark_ref;
const _errs6 = errors;
const _errs7 = errors;
let valid2 = false;
let passing1 = null;
const _errs8 = errors;
if(data1 !== null){
const err6 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/0/properties/benchmark_ref/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
var _valid1 = _errs8 === errors;
if(_valid1){
valid2 = true;
passing1 = 0;
}
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data1 === "string"){
if(!pattern5.test(data1)){
const err7 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/0/properties/benchmark_ref/oneOf/1/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
else {
if(!(formats2.test(data1))){
const err8 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/0/properties/benchmark_ref/oneOf/1/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
else {
const err9 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/0/properties/benchmark_ref/oneOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
}
}
}
var _valid1 = _errs10 === errors;
if(_valid1 && valid2){
valid2 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid2 = true;
passing1 = 1;
}
}
if(!valid2){
const err10 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/0/properties/benchmark_ref/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
else {
errors = _errs7;
if(vErrors !== null){
if(_errs7){
vErrors.length = _errs7;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs6 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.budget !== undefined){
let data2 = data.budget;
const _errs12 = errors;
if(errors === _errs12){
if(data2 && typeof data2 == "object" && !Array.isArray(data2)){
let missing1;
if(((((((((((((data2.schema_version === undefined) && (missing1 = "schema_version")) || ((data2.max_experiments === undefined) && (missing1 = "max_experiments"))) || ((data2.max_parallel_runs === undefined) && (missing1 = "max_parallel_runs"))) || ((data2.max_turns_per_mission === undefined) && (missing1 = "max_turns_per_mission"))) || ((data2.max_repair_turns === undefined) && (missing1 = "max_repair_turns"))) || ((data2.max_wall_seconds === undefined) && (missing1 = "max_wall_seconds"))) || ((data2.max_cpu_seconds === undefined) && (missing1 = "max_cpu_seconds"))) || ((data2.max_memory_mib === undefined) && (missing1 = "max_memory_mib"))) || ((data2.max_output_bytes === undefined) && (missing1 = "max_output_bytes"))) || ((data2.max_cycles_per_day === undefined) && (missing1 = "max_cycles_per_day"))) || ((data2.min_cycle_interval_seconds === undefined) && (missing1 = "min_cycle_interval_seconds"))) || ((data2.cost_enforcement === undefined) && (missing1 = "cost_enforcement"))){
const err11 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/0/properties/budget/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
const _errs14 = errors;
for(const key1 in data2){
if(!(func1.call(schema117.oneOf[0].properties.budget.properties, key1))){
const err12 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/0/properties/budget/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
break;
}
}
if(_errs14 === errors){
if(data2.cost_currency !== undefined){
let data3 = data2.cost_currency;
const _errs15 = errors;
if((typeof data3 !== "string") && (data3 !== null)){
const err13 = {instancePath:instancePath+"/budget/cost_currency",schemaPath:"#/oneOf/0/properties/budget/properties/cost_currency/type",keyword:"type",params:{type: schema117.oneOf[0].properties.budget.properties.cost_currency.type},message:"must be string,null"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
var valid3 = _errs15 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.cost_enforcement !== undefined){
let data4 = data2.cost_enforcement;
const _errs17 = errors;
if(typeof data4 !== "string"){
const err14 = {instancePath:instancePath+"/budget/cost_enforcement",schemaPath:"#/components/schemas/CostEnforcement/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
}
if(!(((data4 === "UNAVAILABLE") || (data4 === "ESTIMATED")) || (data4 === "EXACT"))){
const err15 = {instancePath:instancePath+"/budget/cost_enforcement",schemaPath:"#/components/schemas/CostEnforcement/enum",keyword:"enum",params:{allowedValues: schema118.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err15];
}
else {
vErrors.push(err15);
}
errors++;
}
var valid3 = _errs17 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_cost_decimal !== undefined){
let data5 = data2.max_cost_decimal;
const _errs20 = errors;
const _errs21 = errors;
let valid5 = false;
let passing2 = null;
const _errs22 = errors;
if(data5 !== null){
const err16 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/oneOf/0/properties/budget/properties/max_cost_decimal/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err16];
}
else {
vErrors.push(err16);
}
errors++;
}
var _valid2 = _errs22 === errors;
if(_valid2){
valid5 = true;
passing2 = 0;
}
const _errs24 = errors;
const _errs25 = errors;
if(errors === _errs25){
if(typeof data5 === "string"){
if(func2(data5) > 64){
const err17 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"};
if(vErrors === null){
vErrors = [err17];
}
else {
vErrors.push(err17);
}
errors++;
}
else {
if(func2(data5) < 1){
const err18 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err18];
}
else {
vErrors.push(err18);
}
errors++;
}
else {
if(!pattern38.test(data5)){
const err19 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err19];
}
else {
vErrors.push(err19);
}
errors++;
}
}
}
}
else {
const err20 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err20];
}
else {
vErrors.push(err20);
}
errors++;
}
}
var _valid2 = _errs24 === errors;
if(_valid2 && valid5){
valid5 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid5 = true;
passing2 = 1;
}
}
if(!valid5){
const err21 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/oneOf/0/properties/budget/properties/max_cost_decimal/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err21];
}
else {
vErrors.push(err21);
}
errors++;
}
else {
errors = _errs21;
if(vErrors !== null){
if(_errs21){
vErrors.length = _errs21;
}
else {
vErrors = null;
}
}
}
var valid3 = _errs20 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_cpu_seconds !== undefined){
let data6 = data2.max_cpu_seconds;
const _errs27 = errors;
if(errors === _errs27){
if(typeof data6 === "string"){
if(func2(data6) > 19){
const err22 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_cpu_seconds/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err22];
}
else {
vErrors.push(err22);
}
errors++;
}
else {
if(func2(data6) < 1){
const err23 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_cpu_seconds/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err23];
}
else {
vErrors.push(err23);
}
errors++;
}
else {
if(!pattern27.test(data6)){
const err24 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_cpu_seconds/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err24];
}
else {
vErrors.push(err24);
}
errors++;
}
}
}
}
else {
const err25 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_cpu_seconds/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err25];
}
else {
vErrors.push(err25);
}
errors++;
}
}
var valid3 = _errs27 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_cycles_per_day !== undefined){
let data7 = data2.max_cycles_per_day;
const _errs29 = errors;
if(!((typeof data7 == "number") && (!(data7 % 1) && !isNaN(data7)))){
const err26 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/0/properties/budget/properties/max_cycles_per_day/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err26];
}
else {
vErrors.push(err26);
}
errors++;
}
if(errors === _errs29){
if(typeof data7 == "number"){
if(data7 > 65535 || isNaN(data7)){
const err27 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/0/properties/budget/properties/max_cycles_per_day/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err27];
}
else {
vErrors.push(err27);
}
errors++;
}
else {
if(data7 < 1 || isNaN(data7)){
const err28 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/0/properties/budget/properties/max_cycles_per_day/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err28];
}
else {
vErrors.push(err28);
}
errors++;
}
else {
if(!(formats94.validate(data7))){
const err29 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/0/properties/budget/properties/max_cycles_per_day/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err29];
}
else {
vErrors.push(err29);
}
errors++;
}
}
}
}
}
var valid3 = _errs29 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_experiments !== undefined){
let data8 = data2.max_experiments;
const _errs31 = errors;
if(!((typeof data8 == "number") && (!(data8 % 1) && !isNaN(data8)))){
const err30 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/0/properties/budget/properties/max_experiments/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err30];
}
else {
vErrors.push(err30);
}
errors++;
}
if(errors === _errs31){
if(typeof data8 == "number"){
if(data8 > 4294967295 || isNaN(data8)){
const err31 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/0/properties/budget/properties/max_experiments/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err31];
}
else {
vErrors.push(err31);
}
errors++;
}
else {
if(data8 < 1 || isNaN(data8)){
const err32 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/0/properties/budget/properties/max_experiments/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err32];
}
else {
vErrors.push(err32);
}
errors++;
}
else {
if(!(formats96.validate(data8))){
const err33 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/0/properties/budget/properties/max_experiments/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err33];
}
else {
vErrors.push(err33);
}
errors++;
}
}
}
}
}
var valid3 = _errs31 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_memory_mib !== undefined){
let data9 = data2.max_memory_mib;
const _errs33 = errors;
if(!((typeof data9 == "number") && (!(data9 % 1) && !isNaN(data9)))){
const err34 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/0/properties/budget/properties/max_memory_mib/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err34];
}
else {
vErrors.push(err34);
}
errors++;
}
if(errors === _errs33){
if(typeof data9 == "number"){
if(data9 > 4294967295 || isNaN(data9)){
const err35 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/0/properties/budget/properties/max_memory_mib/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err35];
}
else {
vErrors.push(err35);
}
errors++;
}
else {
if(data9 < 1 || isNaN(data9)){
const err36 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/0/properties/budget/properties/max_memory_mib/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err36];
}
else {
vErrors.push(err36);
}
errors++;
}
else {
if(!(formats96.validate(data9))){
const err37 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/0/properties/budget/properties/max_memory_mib/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err37];
}
else {
vErrors.push(err37);
}
errors++;
}
}
}
}
}
var valid3 = _errs33 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_output_bytes !== undefined){
let data10 = data2.max_output_bytes;
const _errs35 = errors;
if(errors === _errs35){
if(typeof data10 === "string"){
if(func2(data10) > 19){
const err38 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/0/properties/budget/properties/max_output_bytes/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err38];
}
else {
vErrors.push(err38);
}
errors++;
}
else {
if(func2(data10) < 1){
const err39 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/0/properties/budget/properties/max_output_bytes/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err39];
}
else {
vErrors.push(err39);
}
errors++;
}
else {
if(!pattern27.test(data10)){
const err40 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/0/properties/budget/properties/max_output_bytes/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err40];
}
else {
vErrors.push(err40);
}
errors++;
}
}
}
}
else {
const err41 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/0/properties/budget/properties/max_output_bytes/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err41];
}
else {
vErrors.push(err41);
}
errors++;
}
}
var valid3 = _errs35 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_parallel_runs !== undefined){
let data11 = data2.max_parallel_runs;
const _errs37 = errors;
if(!((typeof data11 == "number") && (!(data11 % 1) && !isNaN(data11)))){
const err42 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/0/properties/budget/properties/max_parallel_runs/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err42];
}
else {
vErrors.push(err42);
}
errors++;
}
if(errors === _errs37){
if(typeof data11 == "number"){
if(data11 > 65535 || isNaN(data11)){
const err43 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/0/properties/budget/properties/max_parallel_runs/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err43];
}
else {
vErrors.push(err43);
}
errors++;
}
else {
if(data11 < 1 || isNaN(data11)){
const err44 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/0/properties/budget/properties/max_parallel_runs/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err44];
}
else {
vErrors.push(err44);
}
errors++;
}
else {
if(!(formats94.validate(data11))){
const err45 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/0/properties/budget/properties/max_parallel_runs/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err45];
}
else {
vErrors.push(err45);
}
errors++;
}
}
}
}
}
var valid3 = _errs37 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_repair_turns !== undefined){
let data12 = data2.max_repair_turns;
const _errs39 = errors;
if(!((typeof data12 == "number") && (!(data12 % 1) && !isNaN(data12)))){
const err46 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/0/properties/budget/properties/max_repair_turns/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err46];
}
else {
vErrors.push(err46);
}
errors++;
}
if(errors === _errs39){
if(typeof data12 == "number"){
if(data12 > 65535 || isNaN(data12)){
const err47 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/0/properties/budget/properties/max_repair_turns/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err47];
}
else {
vErrors.push(err47);
}
errors++;
}
else {
if(data12 < 0 || isNaN(data12)){
const err48 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/0/properties/budget/properties/max_repair_turns/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err48];
}
else {
vErrors.push(err48);
}
errors++;
}
else {
if(!(formats94.validate(data12))){
const err49 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/0/properties/budget/properties/max_repair_turns/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err49];
}
else {
vErrors.push(err49);
}
errors++;
}
}
}
}
}
var valid3 = _errs39 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_tokens !== undefined){
let data13 = data2.max_tokens;
const _errs41 = errors;
const _errs42 = errors;
let valid7 = false;
let passing3 = null;
const _errs43 = errors;
if(data13 !== null){
const err50 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/0/properties/budget/properties/max_tokens/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err50];
}
else {
vErrors.push(err50);
}
errors++;
}
var _valid3 = _errs43 === errors;
if(_valid3){
valid7 = true;
passing3 = 0;
}
const _errs45 = errors;
if(errors === _errs45){
if(typeof data13 === "string"){
if(func2(data13) > 19){
const err51 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/0/properties/budget/properties/max_tokens/oneOf/1/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err51];
}
else {
vErrors.push(err51);
}
errors++;
}
else {
if(func2(data13) < 1){
const err52 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/0/properties/budget/properties/max_tokens/oneOf/1/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err52];
}
else {
vErrors.push(err52);
}
errors++;
}
else {
if(!pattern27.test(data13)){
const err53 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/0/properties/budget/properties/max_tokens/oneOf/1/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err53];
}
else {
vErrors.push(err53);
}
errors++;
}
}
}
}
else {
const err54 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/0/properties/budget/properties/max_tokens/oneOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err54];
}
else {
vErrors.push(err54);
}
errors++;
}
}
var _valid3 = _errs45 === errors;
if(_valid3 && valid7){
valid7 = false;
passing3 = [passing3, 1];
}
else {
if(_valid3){
valid7 = true;
passing3 = 1;
}
}
if(!valid7){
const err55 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/0/properties/budget/properties/max_tokens/oneOf",keyword:"oneOf",params:{passingSchemas: passing3},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err55];
}
else {
vErrors.push(err55);
}
errors++;
}
else {
errors = _errs42;
if(vErrors !== null){
if(_errs42){
vErrors.length = _errs42;
}
else {
vErrors = null;
}
}
}
var valid3 = _errs41 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_turns_per_mission !== undefined){
let data14 = data2.max_turns_per_mission;
const _errs47 = errors;
if(!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))){
const err56 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/0/properties/budget/properties/max_turns_per_mission/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err56];
}
else {
vErrors.push(err56);
}
errors++;
}
if(errors === _errs47){
if(typeof data14 == "number"){
if(data14 > 65535 || isNaN(data14)){
const err57 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/0/properties/budget/properties/max_turns_per_mission/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err57];
}
else {
vErrors.push(err57);
}
errors++;
}
else {
if(data14 < 1 || isNaN(data14)){
const err58 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/0/properties/budget/properties/max_turns_per_mission/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err58];
}
else {
vErrors.push(err58);
}
errors++;
}
else {
if(!(formats94.validate(data14))){
const err59 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/0/properties/budget/properties/max_turns_per_mission/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err59];
}
else {
vErrors.push(err59);
}
errors++;
}
}
}
}
}
var valid3 = _errs47 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.max_wall_seconds !== undefined){
let data15 = data2.max_wall_seconds;
const _errs49 = errors;
if(!((typeof data15 == "number") && (!(data15 % 1) && !isNaN(data15)))){
const err60 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_wall_seconds/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err60];
}
else {
vErrors.push(err60);
}
errors++;
}
if(errors === _errs49){
if(typeof data15 == "number"){
if(data15 > 4294967295 || isNaN(data15)){
const err61 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_wall_seconds/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err61];
}
else {
vErrors.push(err61);
}
errors++;
}
else {
if(data15 < 1 || isNaN(data15)){
const err62 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_wall_seconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err62];
}
else {
vErrors.push(err62);
}
errors++;
}
else {
if(!(formats96.validate(data15))){
const err63 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/max_wall_seconds/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err63];
}
else {
vErrors.push(err63);
}
errors++;
}
}
}
}
}
var valid3 = _errs49 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.min_cycle_interval_seconds !== undefined){
let data16 = data2.min_cycle_interval_seconds;
const _errs51 = errors;
if(!((typeof data16 == "number") && (!(data16 % 1) && !isNaN(data16)))){
const err64 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/min_cycle_interval_seconds/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err64];
}
else {
vErrors.push(err64);
}
errors++;
}
if(errors === _errs51){
if(typeof data16 == "number"){
if(data16 > 4294967295 || isNaN(data16)){
const err65 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/min_cycle_interval_seconds/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err65];
}
else {
vErrors.push(err65);
}
errors++;
}
else {
if(data16 < 0 || isNaN(data16)){
const err66 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/min_cycle_interval_seconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err66];
}
else {
vErrors.push(err66);
}
errors++;
}
else {
if(!(formats96.validate(data16))){
const err67 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/0/properties/budget/properties/min_cycle_interval_seconds/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err67];
}
else {
vErrors.push(err67);
}
errors++;
}
}
}
}
}
var valid3 = _errs51 === errors;
}
else {
var valid3 = true;
}
if(valid3){
if(data2.schema_version !== undefined){
let data17 = data2.schema_version;
const _errs53 = errors;
const _errs54 = errors;
if(!((typeof data17 == "number") && (!(data17 % 1) && !isNaN(data17)))){
const err68 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err68];
}
else {
vErrors.push(err68);
}
errors++;
}
if(!(data17 === 1)){
const err69 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err69];
}
else {
vErrors.push(err69);
}
errors++;
}
if(errors === _errs54){
if(typeof data17 == "number"){
if(data17 > 1 || isNaN(data17)){
const err70 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"};
if(vErrors === null){
vErrors = [err70];
}
else {
vErrors.push(err70);
}
errors++;
}
else {
if(data17 < 1 || isNaN(data17)){
const err71 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err71];
}
else {
vErrors.push(err71);
}
errors++;
}
}
}
}
var valid3 = _errs53 === errors;
}
else {
var valid3 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
const err72 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/0/properties/budget/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err72];
}
else {
vErrors.push(err72);
}
errors++;
}
}
var valid1 = _errs12 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.economic_rationale !== undefined){
let data18 = data.economic_rationale;
const _errs56 = errors;
if(errors === _errs56){
if(typeof data18 === "string"){
if(func2(data18) > 8000){
const err73 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/0/properties/economic_rationale/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"};
if(vErrors === null){
vErrors = [err73];
}
else {
vErrors.push(err73);
}
errors++;
}
else {
if(func2(data18) < 1){
const err74 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/0/properties/economic_rationale/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err74];
}
else {
vErrors.push(err74);
}
errors++;
}
}
}
else {
const err75 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/0/properties/economic_rationale/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err75];
}
else {
vErrors.push(err75);
}
errors++;
}
}
var valid1 = _errs56 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.evaluation_policy_id !== undefined){
let data19 = data.evaluation_policy_id;
const _errs58 = errors;
if(errors === _errs58){
if(errors === _errs58){
if(typeof data19 === "string"){
if(!pattern5.test(data19)){
const err76 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/0/properties/evaluation_policy_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err76];
}
else {
vErrors.push(err76);
}
errors++;
}
else {
if(!(formats2.test(data19))){
const err77 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/0/properties/evaluation_policy_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err77];
}
else {
vErrors.push(err77);
}
errors++;
}
}
}
else {
const err78 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/0/properties/evaluation_policy_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err78];
}
else {
vErrors.push(err78);
}
errors++;
}
}
}
var valid1 = _errs58 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.execution_assumptions_id !== undefined){
let data20 = data.execution_assumptions_id;
const _errs60 = errors;
if(errors === _errs60){
if(errors === _errs60){
if(typeof data20 === "string"){
if(!pattern5.test(data20)){
const err79 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/0/properties/execution_assumptions_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err79];
}
else {
vErrors.push(err79);
}
errors++;
}
else {
if(!(formats2.test(data20))){
const err80 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/0/properties/execution_assumptions_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err80];
}
else {
vErrors.push(err80);
}
errors++;
}
}
}
else {
const err81 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/0/properties/execution_assumptions_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err81];
}
else {
vErrors.push(err81);
}
errors++;
}
}
}
var valid1 = _errs60 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.horizon_kind !== undefined){
let data21 = data.horizon_kind;
const _errs62 = errors;
if(typeof data21 !== "string"){
const err82 = {instancePath:instancePath+"/horizon_kind",schemaPath:"#/oneOf/0/properties/horizon_kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err82];
}
else {
vErrors.push(err82);
}
errors++;
}
if(!(data21 === "FIXED_BARS")){
const err83 = {instancePath:instancePath+"/horizon_kind",schemaPath:"#/oneOf/0/properties/horizon_kind/enum",keyword:"enum",params:{allowedValues: schema117.oneOf[0].properties.horizon_kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err83];
}
else {
vErrors.push(err83);
}
errors++;
}
var valid1 = _errs62 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.horizon_value !== undefined){
let data22 = data.horizon_value;
const _errs64 = errors;
if(errors === _errs64){
if(typeof data22 === "string"){
if(func2(data22) > 19){
const err84 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/0/properties/horizon_value/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err84];
}
else {
vErrors.push(err84);
}
errors++;
}
else {
if(func2(data22) < 1){
const err85 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/0/properties/horizon_value/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err85];
}
else {
vErrors.push(err85);
}
errors++;
}
else {
if(!pattern27.test(data22)){
const err86 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/0/properties/horizon_value/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err86];
}
else {
vErrors.push(err86);
}
errors++;
}
}
}
}
else {
const err87 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/0/properties/horizon_value/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err87];
}
else {
vErrors.push(err87);
}
errors++;
}
}
var valid1 = _errs64 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.hypothesis !== undefined){
let data23 = data.hypothesis;
const _errs66 = errors;
if(errors === _errs66){
if(typeof data23 === "string"){
if(func2(data23) > 8000){
const err88 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/0/properties/hypothesis/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"};
if(vErrors === null){
vErrors = [err88];
}
else {
vErrors.push(err88);
}
errors++;
}
else {
if(func2(data23) < 1){
const err89 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/0/properties/hypothesis/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err89];
}
else {
vErrors.push(err89);
}
errors++;
}
}
}
else {
const err90 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/0/properties/hypothesis/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err90];
}
else {
vErrors.push(err90);
}
errors++;
}
}
var valid1 = _errs66 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.stop_rule !== undefined){
let data24 = data.stop_rule;
const _errs68 = errors;
if(errors === _errs68){
if(data24 && typeof data24 == "object" && !Array.isArray(data24)){
let missing2;
if(((((data24.schema_version === undefined) && (missing2 = "schema_version")) || ((data24.stop_on_qualified_count === undefined) && (missing2 = "stop_on_qualified_count"))) || ((data24.stop_on_budget === undefined) && (missing2 = "stop_on_budget"))) || ((data24.stop_on_invalid_data === undefined) && (missing2 = "stop_on_invalid_data"))){
const err91 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/0/properties/stop_rule/required",keyword:"required",params:{missingProperty: missing2},message:"must have required property '"+missing2+"'"};
if(vErrors === null){
vErrors = [err91];
}
else {
vErrors.push(err91);
}
errors++;
}
else {
const _errs70 = errors;
for(const key2 in data24){
if(!(((((key2 === "schema_version") || (key2 === "stop_on_budget")) || (key2 === "stop_on_invalid_data")) || (key2 === "stop_on_no_improvement_trials")) || (key2 === "stop_on_qualified_count"))){
const err92 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/0/properties/stop_rule/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key2},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err92];
}
else {
vErrors.push(err92);
}
errors++;
break;
}
}
if(_errs70 === errors){
if(data24.schema_version !== undefined){
let data25 = data24.schema_version;
const _errs71 = errors;
const _errs72 = errors;
if(!((typeof data25 == "number") && (!(data25 % 1) && !isNaN(data25)))){
const err93 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err93];
}
else {
vErrors.push(err93);
}
errors++;
}
if(!(data25 === 1)){
const err94 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err94];
}
else {
vErrors.push(err94);
}
errors++;
}
if(errors === _errs72){
if(typeof data25 == "number"){
if(data25 > 1 || isNaN(data25)){
const err95 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"};
if(vErrors === null){
vErrors = [err95];
}
else {
vErrors.push(err95);
}
errors++;
}
else {
if(data25 < 1 || isNaN(data25)){
const err96 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err96];
}
else {
vErrors.push(err96);
}
errors++;
}
}
}
}
var valid9 = _errs71 === errors;
}
else {
var valid9 = true;
}
if(valid9){
if(data24.stop_on_budget !== undefined){
const _errs74 = errors;
if(typeof data24.stop_on_budget !== "boolean"){
const err97 = {instancePath:instancePath+"/stop_rule/stop_on_budget",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_budget/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err97];
}
else {
vErrors.push(err97);
}
errors++;
}
var valid9 = _errs74 === errors;
}
else {
var valid9 = true;
}
if(valid9){
if(data24.stop_on_invalid_data !== undefined){
const _errs76 = errors;
if(typeof data24.stop_on_invalid_data !== "boolean"){
const err98 = {instancePath:instancePath+"/stop_rule/stop_on_invalid_data",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_invalid_data/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err98];
}
else {
vErrors.push(err98);
}
errors++;
}
var valid9 = _errs76 === errors;
}
else {
var valid9 = true;
}
if(valid9){
if(data24.stop_on_no_improvement_trials !== undefined){
let data28 = data24.stop_on_no_improvement_trials;
const _errs78 = errors;
if((!((typeof data28 == "number") && (!(data28 % 1) && !isNaN(data28)))) && (data28 !== null)){
const err99 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_no_improvement_trials/type",keyword:"type",params:{type: schema117.oneOf[0].properties.stop_rule.properties.stop_on_no_improvement_trials.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err99];
}
else {
vErrors.push(err99);
}
errors++;
}
if(errors === _errs78){
if(typeof data28 == "number"){
if(data28 > 65535 || isNaN(data28)){
const err100 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_no_improvement_trials/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err100];
}
else {
vErrors.push(err100);
}
errors++;
}
else {
if(data28 < 1 || isNaN(data28)){
const err101 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_no_improvement_trials/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err101];
}
else {
vErrors.push(err101);
}
errors++;
}
else {
if(!(formats94.validate(data28))){
const err102 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_no_improvement_trials/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err102];
}
else {
vErrors.push(err102);
}
errors++;
}
}
}
}
}
var valid9 = _errs78 === errors;
}
else {
var valid9 = true;
}
if(valid9){
if(data24.stop_on_qualified_count !== undefined){
let data29 = data24.stop_on_qualified_count;
const _errs80 = errors;
if(!((typeof data29 == "number") && (!(data29 % 1) && !isNaN(data29)))){
const err103 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_qualified_count/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err103];
}
else {
vErrors.push(err103);
}
errors++;
}
if(errors === _errs80){
if(typeof data29 == "number"){
if(data29 > 65535 || isNaN(data29)){
const err104 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_qualified_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err104];
}
else {
vErrors.push(err104);
}
errors++;
}
else {
if(data29 < 1 || isNaN(data29)){
const err105 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_qualified_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err105];
}
else {
vErrors.push(err105);
}
errors++;
}
else {
if(!(formats94.validate(data29))){
const err106 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/0/properties/stop_rule/properties/stop_on_qualified_count/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err106];
}
else {
vErrors.push(err106);
}
errors++;
}
}
}
}
}
var valid9 = _errs80 === errors;
}
else {
var valid9 = true;
}
}
}
}
}
}
}
}
else {
const err107 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/0/properties/stop_rule/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err107];
}
else {
vErrors.push(err107);
}
errors++;
}
}
var valid1 = _errs68 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.target_kind !== undefined){
let data30 = data.target_kind;
const _errs82 = errors;
if(typeof data30 !== "string"){
const err108 = {instancePath:instancePath+"/target_kind",schemaPath:"#/oneOf/0/properties/target_kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err108];
}
else {
vErrors.push(err108);
}
errors++;
}
if(!((data30 === "SCORE") || (data30 === "EXPECTED_RETURN"))){
const err109 = {instancePath:instancePath+"/target_kind",schemaPath:"#/oneOf/0/properties/target_kind/enum",keyword:"enum",params:{allowedValues: schema117.oneOf[0].properties.target_kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err109];
}
else {
vErrors.push(err109);
}
errors++;
}
var valid1 = _errs82 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.universe_version_id !== undefined){
let data31 = data.universe_version_id;
const _errs84 = errors;
if(errors === _errs84){
if(errors === _errs84){
if(typeof data31 === "string"){
if(!pattern5.test(data31)){
const err110 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/0/properties/universe_version_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err110];
}
else {
vErrors.push(err110);
}
errors++;
}
else {
if(!(formats2.test(data31))){
const err111 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/0/properties/universe_version_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err111];
}
else {
vErrors.push(err111);
}
errors++;
}
}
}
else {
const err112 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/0/properties/universe_version_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err112];
}
else {
vErrors.push(err112);
}
errors++;
}
}
}
var valid1 = _errs84 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
const err113 = {instancePath,schemaPath:"#/oneOf/0/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err113];
}
else {
vErrors.push(err113);
}
errors++;
}
}
var _valid0 = _errs1 === errors;
if(_valid0){
valid0 = true;
passing0 = 0;
var props0 = true;
}
const _errs86 = errors;
if(errors === _errs86){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing3;
if((((((((((((data.hypothesis === undefined) && (missing3 = "hypothesis")) || ((data.economic_rationale === undefined) && (missing3 = "economic_rationale"))) || ((data.universe_version_id === undefined) && (missing3 = "universe_version_id"))) || ((data.target_kind === undefined) && (missing3 = "target_kind"))) || ((data.horizon_kind === undefined) && (missing3 = "horizon_kind"))) || ((data.base_currency === undefined) && (missing3 = "base_currency"))) || ((data.evaluation_policy_id === undefined) && (missing3 = "evaluation_policy_id"))) || ((data.execution_assumptions_id === undefined) && (missing3 = "execution_assumptions_id"))) || ((data.budget === undefined) && (missing3 = "budget"))) || ((data.stop_rule === undefined) && (missing3 = "stop_rule"))) || ((data.horizon_value === undefined) && (missing3 = "horizon_value"))){
const err114 = {instancePath,schemaPath:"#/oneOf/1/required",keyword:"required",params:{missingProperty: missing3},message:"must have required property '"+missing3+"'"};
if(vErrors === null){
vErrors = [err114];
}
else {
vErrors.push(err114);
}
errors++;
}
else {
const _errs88 = errors;
for(const key3 in data){
if(!(func1.call(schema117.oneOf[1].properties, key3))){
const err115 = {instancePath,schemaPath:"#/oneOf/1/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key3},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err115];
}
else {
vErrors.push(err115);
}
errors++;
break;
}
}
if(_errs88 === errors){
if(data.base_currency !== undefined){
let data32 = data.base_currency;
const _errs89 = errors;
if(errors === _errs89){
if(typeof data32 === "string"){
if(func2(data32) > 3){
const err116 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/1/properties/base_currency/maxLength",keyword:"maxLength",params:{limit: 3},message:"must NOT have more than 3 characters"};
if(vErrors === null){
vErrors = [err116];
}
else {
vErrors.push(err116);
}
errors++;
}
else {
if(func2(data32) < 3){
const err117 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/1/properties/base_currency/minLength",keyword:"minLength",params:{limit: 3},message:"must NOT have fewer than 3 characters"};
if(vErrors === null){
vErrors = [err117];
}
else {
vErrors.push(err117);
}
errors++;
}
else {
if(!pattern36.test(data32)){
const err118 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/1/properties/base_currency/pattern",keyword:"pattern",params:{pattern: "^[A-Z]{3}$"},message:"must match pattern \""+"^[A-Z]{3}$"+"\""};
if(vErrors === null){
vErrors = [err118];
}
else {
vErrors.push(err118);
}
errors++;
}
}
}
}
else {
const err119 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/1/properties/base_currency/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err119];
}
else {
vErrors.push(err119);
}
errors++;
}
}
var valid11 = _errs89 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.benchmark_ref !== undefined){
let data33 = data.benchmark_ref;
const _errs91 = errors;
const _errs92 = errors;
let valid12 = false;
let passing4 = null;
const _errs93 = errors;
if(data33 !== null){
const err120 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/1/properties/benchmark_ref/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err120];
}
else {
vErrors.push(err120);
}
errors++;
}
var _valid4 = _errs93 === errors;
if(_valid4){
valid12 = true;
passing4 = 0;
}
const _errs95 = errors;
if(errors === _errs95){
if(errors === _errs95){
if(typeof data33 === "string"){
if(!pattern5.test(data33)){
const err121 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/1/properties/benchmark_ref/oneOf/1/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err121];
}
else {
vErrors.push(err121);
}
errors++;
}
else {
if(!(formats2.test(data33))){
const err122 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/1/properties/benchmark_ref/oneOf/1/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err122];
}
else {
vErrors.push(err122);
}
errors++;
}
}
}
else {
const err123 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/1/properties/benchmark_ref/oneOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err123];
}
else {
vErrors.push(err123);
}
errors++;
}
}
}
var _valid4 = _errs95 === errors;
if(_valid4 && valid12){
valid12 = false;
passing4 = [passing4, 1];
}
else {
if(_valid4){
valid12 = true;
passing4 = 1;
}
}
if(!valid12){
const err124 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/1/properties/benchmark_ref/oneOf",keyword:"oneOf",params:{passingSchemas: passing4},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err124];
}
else {
vErrors.push(err124);
}
errors++;
}
else {
errors = _errs92;
if(vErrors !== null){
if(_errs92){
vErrors.length = _errs92;
}
else {
vErrors = null;
}
}
}
var valid11 = _errs91 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.budget !== undefined){
let data34 = data.budget;
const _errs97 = errors;
if(errors === _errs97){
if(data34 && typeof data34 == "object" && !Array.isArray(data34)){
let missing4;
if(((((((((((((data34.schema_version === undefined) && (missing4 = "schema_version")) || ((data34.max_experiments === undefined) && (missing4 = "max_experiments"))) || ((data34.max_parallel_runs === undefined) && (missing4 = "max_parallel_runs"))) || ((data34.max_turns_per_mission === undefined) && (missing4 = "max_turns_per_mission"))) || ((data34.max_repair_turns === undefined) && (missing4 = "max_repair_turns"))) || ((data34.max_wall_seconds === undefined) && (missing4 = "max_wall_seconds"))) || ((data34.max_cpu_seconds === undefined) && (missing4 = "max_cpu_seconds"))) || ((data34.max_memory_mib === undefined) && (missing4 = "max_memory_mib"))) || ((data34.max_output_bytes === undefined) && (missing4 = "max_output_bytes"))) || ((data34.max_cycles_per_day === undefined) && (missing4 = "max_cycles_per_day"))) || ((data34.min_cycle_interval_seconds === undefined) && (missing4 = "min_cycle_interval_seconds"))) || ((data34.cost_enforcement === undefined) && (missing4 = "cost_enforcement"))){
const err125 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/1/properties/budget/required",keyword:"required",params:{missingProperty: missing4},message:"must have required property '"+missing4+"'"};
if(vErrors === null){
vErrors = [err125];
}
else {
vErrors.push(err125);
}
errors++;
}
else {
const _errs99 = errors;
for(const key4 in data34){
if(!(func1.call(schema117.oneOf[1].properties.budget.properties, key4))){
const err126 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/1/properties/budget/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key4},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err126];
}
else {
vErrors.push(err126);
}
errors++;
break;
}
}
if(_errs99 === errors){
if(data34.cost_currency !== undefined){
let data35 = data34.cost_currency;
const _errs100 = errors;
if((typeof data35 !== "string") && (data35 !== null)){
const err127 = {instancePath:instancePath+"/budget/cost_currency",schemaPath:"#/oneOf/1/properties/budget/properties/cost_currency/type",keyword:"type",params:{type: schema117.oneOf[1].properties.budget.properties.cost_currency.type},message:"must be string,null"};
if(vErrors === null){
vErrors = [err127];
}
else {
vErrors.push(err127);
}
errors++;
}
var valid13 = _errs100 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.cost_enforcement !== undefined){
let data36 = data34.cost_enforcement;
const _errs102 = errors;
if(typeof data36 !== "string"){
const err128 = {instancePath:instancePath+"/budget/cost_enforcement",schemaPath:"#/components/schemas/CostEnforcement/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err128];
}
else {
vErrors.push(err128);
}
errors++;
}
if(!(((data36 === "UNAVAILABLE") || (data36 === "ESTIMATED")) || (data36 === "EXACT"))){
const err129 = {instancePath:instancePath+"/budget/cost_enforcement",schemaPath:"#/components/schemas/CostEnforcement/enum",keyword:"enum",params:{allowedValues: schema118.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err129];
}
else {
vErrors.push(err129);
}
errors++;
}
var valid13 = _errs102 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_cost_decimal !== undefined){
let data37 = data34.max_cost_decimal;
const _errs105 = errors;
const _errs106 = errors;
let valid15 = false;
let passing5 = null;
const _errs107 = errors;
if(data37 !== null){
const err130 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/oneOf/1/properties/budget/properties/max_cost_decimal/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err130];
}
else {
vErrors.push(err130);
}
errors++;
}
var _valid5 = _errs107 === errors;
if(_valid5){
valid15 = true;
passing5 = 0;
}
const _errs109 = errors;
const _errs110 = errors;
if(errors === _errs110){
if(typeof data37 === "string"){
if(func2(data37) > 64){
const err131 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"};
if(vErrors === null){
vErrors = [err131];
}
else {
vErrors.push(err131);
}
errors++;
}
else {
if(func2(data37) < 1){
const err132 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err132];
}
else {
vErrors.push(err132);
}
errors++;
}
else {
if(!pattern38.test(data37)){
const err133 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err133];
}
else {
vErrors.push(err133);
}
errors++;
}
}
}
}
else {
const err134 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err134];
}
else {
vErrors.push(err134);
}
errors++;
}
}
var _valid5 = _errs109 === errors;
if(_valid5 && valid15){
valid15 = false;
passing5 = [passing5, 1];
}
else {
if(_valid5){
valid15 = true;
passing5 = 1;
}
}
if(!valid15){
const err135 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/oneOf/1/properties/budget/properties/max_cost_decimal/oneOf",keyword:"oneOf",params:{passingSchemas: passing5},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err135];
}
else {
vErrors.push(err135);
}
errors++;
}
else {
errors = _errs106;
if(vErrors !== null){
if(_errs106){
vErrors.length = _errs106;
}
else {
vErrors = null;
}
}
}
var valid13 = _errs105 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_cpu_seconds !== undefined){
let data38 = data34.max_cpu_seconds;
const _errs112 = errors;
if(errors === _errs112){
if(typeof data38 === "string"){
if(func2(data38) > 19){
const err136 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_cpu_seconds/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err136];
}
else {
vErrors.push(err136);
}
errors++;
}
else {
if(func2(data38) < 1){
const err137 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_cpu_seconds/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err137];
}
else {
vErrors.push(err137);
}
errors++;
}
else {
if(!pattern27.test(data38)){
const err138 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_cpu_seconds/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err138];
}
else {
vErrors.push(err138);
}
errors++;
}
}
}
}
else {
const err139 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_cpu_seconds/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err139];
}
else {
vErrors.push(err139);
}
errors++;
}
}
var valid13 = _errs112 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_cycles_per_day !== undefined){
let data39 = data34.max_cycles_per_day;
const _errs114 = errors;
if(!((typeof data39 == "number") && (!(data39 % 1) && !isNaN(data39)))){
const err140 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/1/properties/budget/properties/max_cycles_per_day/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err140];
}
else {
vErrors.push(err140);
}
errors++;
}
if(errors === _errs114){
if(typeof data39 == "number"){
if(data39 > 65535 || isNaN(data39)){
const err141 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/1/properties/budget/properties/max_cycles_per_day/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err141];
}
else {
vErrors.push(err141);
}
errors++;
}
else {
if(data39 < 1 || isNaN(data39)){
const err142 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/1/properties/budget/properties/max_cycles_per_day/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err142];
}
else {
vErrors.push(err142);
}
errors++;
}
else {
if(!(formats94.validate(data39))){
const err143 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/1/properties/budget/properties/max_cycles_per_day/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err143];
}
else {
vErrors.push(err143);
}
errors++;
}
}
}
}
}
var valid13 = _errs114 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_experiments !== undefined){
let data40 = data34.max_experiments;
const _errs116 = errors;
if(!((typeof data40 == "number") && (!(data40 % 1) && !isNaN(data40)))){
const err144 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/1/properties/budget/properties/max_experiments/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err144];
}
else {
vErrors.push(err144);
}
errors++;
}
if(errors === _errs116){
if(typeof data40 == "number"){
if(data40 > 4294967295 || isNaN(data40)){
const err145 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/1/properties/budget/properties/max_experiments/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err145];
}
else {
vErrors.push(err145);
}
errors++;
}
else {
if(data40 < 1 || isNaN(data40)){
const err146 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/1/properties/budget/properties/max_experiments/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err146];
}
else {
vErrors.push(err146);
}
errors++;
}
else {
if(!(formats96.validate(data40))){
const err147 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/1/properties/budget/properties/max_experiments/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err147];
}
else {
vErrors.push(err147);
}
errors++;
}
}
}
}
}
var valid13 = _errs116 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_memory_mib !== undefined){
let data41 = data34.max_memory_mib;
const _errs118 = errors;
if(!((typeof data41 == "number") && (!(data41 % 1) && !isNaN(data41)))){
const err148 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/1/properties/budget/properties/max_memory_mib/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err148];
}
else {
vErrors.push(err148);
}
errors++;
}
if(errors === _errs118){
if(typeof data41 == "number"){
if(data41 > 4294967295 || isNaN(data41)){
const err149 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/1/properties/budget/properties/max_memory_mib/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err149];
}
else {
vErrors.push(err149);
}
errors++;
}
else {
if(data41 < 1 || isNaN(data41)){
const err150 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/1/properties/budget/properties/max_memory_mib/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err150];
}
else {
vErrors.push(err150);
}
errors++;
}
else {
if(!(formats96.validate(data41))){
const err151 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/1/properties/budget/properties/max_memory_mib/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err151];
}
else {
vErrors.push(err151);
}
errors++;
}
}
}
}
}
var valid13 = _errs118 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_output_bytes !== undefined){
let data42 = data34.max_output_bytes;
const _errs120 = errors;
if(errors === _errs120){
if(typeof data42 === "string"){
if(func2(data42) > 19){
const err152 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/1/properties/budget/properties/max_output_bytes/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err152];
}
else {
vErrors.push(err152);
}
errors++;
}
else {
if(func2(data42) < 1){
const err153 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/1/properties/budget/properties/max_output_bytes/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err153];
}
else {
vErrors.push(err153);
}
errors++;
}
else {
if(!pattern27.test(data42)){
const err154 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/1/properties/budget/properties/max_output_bytes/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err154];
}
else {
vErrors.push(err154);
}
errors++;
}
}
}
}
else {
const err155 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/1/properties/budget/properties/max_output_bytes/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err155];
}
else {
vErrors.push(err155);
}
errors++;
}
}
var valid13 = _errs120 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_parallel_runs !== undefined){
let data43 = data34.max_parallel_runs;
const _errs122 = errors;
if(!((typeof data43 == "number") && (!(data43 % 1) && !isNaN(data43)))){
const err156 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/1/properties/budget/properties/max_parallel_runs/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err156];
}
else {
vErrors.push(err156);
}
errors++;
}
if(errors === _errs122){
if(typeof data43 == "number"){
if(data43 > 65535 || isNaN(data43)){
const err157 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/1/properties/budget/properties/max_parallel_runs/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err157];
}
else {
vErrors.push(err157);
}
errors++;
}
else {
if(data43 < 1 || isNaN(data43)){
const err158 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/1/properties/budget/properties/max_parallel_runs/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err158];
}
else {
vErrors.push(err158);
}
errors++;
}
else {
if(!(formats94.validate(data43))){
const err159 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/1/properties/budget/properties/max_parallel_runs/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err159];
}
else {
vErrors.push(err159);
}
errors++;
}
}
}
}
}
var valid13 = _errs122 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_repair_turns !== undefined){
let data44 = data34.max_repair_turns;
const _errs124 = errors;
if(!((typeof data44 == "number") && (!(data44 % 1) && !isNaN(data44)))){
const err160 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/1/properties/budget/properties/max_repair_turns/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err160];
}
else {
vErrors.push(err160);
}
errors++;
}
if(errors === _errs124){
if(typeof data44 == "number"){
if(data44 > 65535 || isNaN(data44)){
const err161 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/1/properties/budget/properties/max_repair_turns/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err161];
}
else {
vErrors.push(err161);
}
errors++;
}
else {
if(data44 < 0 || isNaN(data44)){
const err162 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/1/properties/budget/properties/max_repair_turns/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err162];
}
else {
vErrors.push(err162);
}
errors++;
}
else {
if(!(formats94.validate(data44))){
const err163 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/1/properties/budget/properties/max_repair_turns/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err163];
}
else {
vErrors.push(err163);
}
errors++;
}
}
}
}
}
var valid13 = _errs124 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_tokens !== undefined){
let data45 = data34.max_tokens;
const _errs126 = errors;
const _errs127 = errors;
let valid17 = false;
let passing6 = null;
const _errs128 = errors;
if(data45 !== null){
const err164 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/1/properties/budget/properties/max_tokens/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err164];
}
else {
vErrors.push(err164);
}
errors++;
}
var _valid6 = _errs128 === errors;
if(_valid6){
valid17 = true;
passing6 = 0;
}
const _errs130 = errors;
if(errors === _errs130){
if(typeof data45 === "string"){
if(func2(data45) > 19){
const err165 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/1/properties/budget/properties/max_tokens/oneOf/1/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err165];
}
else {
vErrors.push(err165);
}
errors++;
}
else {
if(func2(data45) < 1){
const err166 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/1/properties/budget/properties/max_tokens/oneOf/1/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err166];
}
else {
vErrors.push(err166);
}
errors++;
}
else {
if(!pattern27.test(data45)){
const err167 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/1/properties/budget/properties/max_tokens/oneOf/1/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err167];
}
else {
vErrors.push(err167);
}
errors++;
}
}
}
}
else {
const err168 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/1/properties/budget/properties/max_tokens/oneOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err168];
}
else {
vErrors.push(err168);
}
errors++;
}
}
var _valid6 = _errs130 === errors;
if(_valid6 && valid17){
valid17 = false;
passing6 = [passing6, 1];
}
else {
if(_valid6){
valid17 = true;
passing6 = 1;
}
}
if(!valid17){
const err169 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/1/properties/budget/properties/max_tokens/oneOf",keyword:"oneOf",params:{passingSchemas: passing6},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err169];
}
else {
vErrors.push(err169);
}
errors++;
}
else {
errors = _errs127;
if(vErrors !== null){
if(_errs127){
vErrors.length = _errs127;
}
else {
vErrors = null;
}
}
}
var valid13 = _errs126 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_turns_per_mission !== undefined){
let data46 = data34.max_turns_per_mission;
const _errs132 = errors;
if(!((typeof data46 == "number") && (!(data46 % 1) && !isNaN(data46)))){
const err170 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/1/properties/budget/properties/max_turns_per_mission/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err170];
}
else {
vErrors.push(err170);
}
errors++;
}
if(errors === _errs132){
if(typeof data46 == "number"){
if(data46 > 65535 || isNaN(data46)){
const err171 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/1/properties/budget/properties/max_turns_per_mission/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err171];
}
else {
vErrors.push(err171);
}
errors++;
}
else {
if(data46 < 1 || isNaN(data46)){
const err172 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/1/properties/budget/properties/max_turns_per_mission/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err172];
}
else {
vErrors.push(err172);
}
errors++;
}
else {
if(!(formats94.validate(data46))){
const err173 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/1/properties/budget/properties/max_turns_per_mission/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err173];
}
else {
vErrors.push(err173);
}
errors++;
}
}
}
}
}
var valid13 = _errs132 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.max_wall_seconds !== undefined){
let data47 = data34.max_wall_seconds;
const _errs134 = errors;
if(!((typeof data47 == "number") && (!(data47 % 1) && !isNaN(data47)))){
const err174 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_wall_seconds/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err174];
}
else {
vErrors.push(err174);
}
errors++;
}
if(errors === _errs134){
if(typeof data47 == "number"){
if(data47 > 4294967295 || isNaN(data47)){
const err175 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_wall_seconds/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err175];
}
else {
vErrors.push(err175);
}
errors++;
}
else {
if(data47 < 1 || isNaN(data47)){
const err176 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_wall_seconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err176];
}
else {
vErrors.push(err176);
}
errors++;
}
else {
if(!(formats96.validate(data47))){
const err177 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/max_wall_seconds/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err177];
}
else {
vErrors.push(err177);
}
errors++;
}
}
}
}
}
var valid13 = _errs134 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.min_cycle_interval_seconds !== undefined){
let data48 = data34.min_cycle_interval_seconds;
const _errs136 = errors;
if(!((typeof data48 == "number") && (!(data48 % 1) && !isNaN(data48)))){
const err178 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/min_cycle_interval_seconds/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err178];
}
else {
vErrors.push(err178);
}
errors++;
}
if(errors === _errs136){
if(typeof data48 == "number"){
if(data48 > 4294967295 || isNaN(data48)){
const err179 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/min_cycle_interval_seconds/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err179];
}
else {
vErrors.push(err179);
}
errors++;
}
else {
if(data48 < 0 || isNaN(data48)){
const err180 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/min_cycle_interval_seconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err180];
}
else {
vErrors.push(err180);
}
errors++;
}
else {
if(!(formats96.validate(data48))){
const err181 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/1/properties/budget/properties/min_cycle_interval_seconds/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err181];
}
else {
vErrors.push(err181);
}
errors++;
}
}
}
}
}
var valid13 = _errs136 === errors;
}
else {
var valid13 = true;
}
if(valid13){
if(data34.schema_version !== undefined){
let data49 = data34.schema_version;
const _errs138 = errors;
const _errs139 = errors;
if(!((typeof data49 == "number") && (!(data49 % 1) && !isNaN(data49)))){
const err182 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err182];
}
else {
vErrors.push(err182);
}
errors++;
}
if(!(data49 === 1)){
const err183 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err183];
}
else {
vErrors.push(err183);
}
errors++;
}
if(errors === _errs139){
if(typeof data49 == "number"){
if(data49 > 1 || isNaN(data49)){
const err184 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"};
if(vErrors === null){
vErrors = [err184];
}
else {
vErrors.push(err184);
}
errors++;
}
else {
if(data49 < 1 || isNaN(data49)){
const err185 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err185];
}
else {
vErrors.push(err185);
}
errors++;
}
}
}
}
var valid13 = _errs138 === errors;
}
else {
var valid13 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
const err186 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/1/properties/budget/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err186];
}
else {
vErrors.push(err186);
}
errors++;
}
}
var valid11 = _errs97 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.economic_rationale !== undefined){
let data50 = data.economic_rationale;
const _errs141 = errors;
if(errors === _errs141){
if(typeof data50 === "string"){
if(func2(data50) > 8000){
const err187 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/1/properties/economic_rationale/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"};
if(vErrors === null){
vErrors = [err187];
}
else {
vErrors.push(err187);
}
errors++;
}
else {
if(func2(data50) < 1){
const err188 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/1/properties/economic_rationale/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err188];
}
else {
vErrors.push(err188);
}
errors++;
}
}
}
else {
const err189 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/1/properties/economic_rationale/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err189];
}
else {
vErrors.push(err189);
}
errors++;
}
}
var valid11 = _errs141 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.evaluation_policy_id !== undefined){
let data51 = data.evaluation_policy_id;
const _errs143 = errors;
if(errors === _errs143){
if(errors === _errs143){
if(typeof data51 === "string"){
if(!pattern5.test(data51)){
const err190 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/1/properties/evaluation_policy_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err190];
}
else {
vErrors.push(err190);
}
errors++;
}
else {
if(!(formats2.test(data51))){
const err191 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/1/properties/evaluation_policy_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err191];
}
else {
vErrors.push(err191);
}
errors++;
}
}
}
else {
const err192 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/1/properties/evaluation_policy_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err192];
}
else {
vErrors.push(err192);
}
errors++;
}
}
}
var valid11 = _errs143 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.execution_assumptions_id !== undefined){
let data52 = data.execution_assumptions_id;
const _errs145 = errors;
if(errors === _errs145){
if(errors === _errs145){
if(typeof data52 === "string"){
if(!pattern5.test(data52)){
const err193 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/1/properties/execution_assumptions_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err193];
}
else {
vErrors.push(err193);
}
errors++;
}
else {
if(!(formats2.test(data52))){
const err194 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/1/properties/execution_assumptions_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err194];
}
else {
vErrors.push(err194);
}
errors++;
}
}
}
else {
const err195 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/1/properties/execution_assumptions_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err195];
}
else {
vErrors.push(err195);
}
errors++;
}
}
}
var valid11 = _errs145 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.horizon_kind !== undefined){
let data53 = data.horizon_kind;
const _errs147 = errors;
if(typeof data53 !== "string"){
const err196 = {instancePath:instancePath+"/horizon_kind",schemaPath:"#/oneOf/1/properties/horizon_kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err196];
}
else {
vErrors.push(err196);
}
errors++;
}
if(!(data53 === "FIXED_DURATION")){
const err197 = {instancePath:instancePath+"/horizon_kind",schemaPath:"#/oneOf/1/properties/horizon_kind/enum",keyword:"enum",params:{allowedValues: schema117.oneOf[1].properties.horizon_kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err197];
}
else {
vErrors.push(err197);
}
errors++;
}
var valid11 = _errs147 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.horizon_value !== undefined){
let data54 = data.horizon_value;
const _errs149 = errors;
if(errors === _errs149){
if(typeof data54 === "string"){
if(func2(data54) > 19){
const err198 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/1/properties/horizon_value/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err198];
}
else {
vErrors.push(err198);
}
errors++;
}
else {
if(func2(data54) < 1){
const err199 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/1/properties/horizon_value/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err199];
}
else {
vErrors.push(err199);
}
errors++;
}
else {
if(!pattern27.test(data54)){
const err200 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/1/properties/horizon_value/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err200];
}
else {
vErrors.push(err200);
}
errors++;
}
}
}
}
else {
const err201 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/1/properties/horizon_value/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err201];
}
else {
vErrors.push(err201);
}
errors++;
}
}
var valid11 = _errs149 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.hypothesis !== undefined){
let data55 = data.hypothesis;
const _errs151 = errors;
if(errors === _errs151){
if(typeof data55 === "string"){
if(func2(data55) > 8000){
const err202 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/1/properties/hypothesis/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"};
if(vErrors === null){
vErrors = [err202];
}
else {
vErrors.push(err202);
}
errors++;
}
else {
if(func2(data55) < 1){
const err203 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/1/properties/hypothesis/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err203];
}
else {
vErrors.push(err203);
}
errors++;
}
}
}
else {
const err204 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/1/properties/hypothesis/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err204];
}
else {
vErrors.push(err204);
}
errors++;
}
}
var valid11 = _errs151 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.stop_rule !== undefined){
let data56 = data.stop_rule;
const _errs153 = errors;
if(errors === _errs153){
if(data56 && typeof data56 == "object" && !Array.isArray(data56)){
let missing5;
if(((((data56.schema_version === undefined) && (missing5 = "schema_version")) || ((data56.stop_on_qualified_count === undefined) && (missing5 = "stop_on_qualified_count"))) || ((data56.stop_on_budget === undefined) && (missing5 = "stop_on_budget"))) || ((data56.stop_on_invalid_data === undefined) && (missing5 = "stop_on_invalid_data"))){
const err205 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/1/properties/stop_rule/required",keyword:"required",params:{missingProperty: missing5},message:"must have required property '"+missing5+"'"};
if(vErrors === null){
vErrors = [err205];
}
else {
vErrors.push(err205);
}
errors++;
}
else {
const _errs155 = errors;
for(const key5 in data56){
if(!(((((key5 === "schema_version") || (key5 === "stop_on_budget")) || (key5 === "stop_on_invalid_data")) || (key5 === "stop_on_no_improvement_trials")) || (key5 === "stop_on_qualified_count"))){
const err206 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/1/properties/stop_rule/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key5},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err206];
}
else {
vErrors.push(err206);
}
errors++;
break;
}
}
if(_errs155 === errors){
if(data56.schema_version !== undefined){
let data57 = data56.schema_version;
const _errs156 = errors;
const _errs157 = errors;
if(!((typeof data57 == "number") && (!(data57 % 1) && !isNaN(data57)))){
const err207 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err207];
}
else {
vErrors.push(err207);
}
errors++;
}
if(!(data57 === 1)){
const err208 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err208];
}
else {
vErrors.push(err208);
}
errors++;
}
if(errors === _errs157){
if(typeof data57 == "number"){
if(data57 > 1 || isNaN(data57)){
const err209 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"};
if(vErrors === null){
vErrors = [err209];
}
else {
vErrors.push(err209);
}
errors++;
}
else {
if(data57 < 1 || isNaN(data57)){
const err210 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err210];
}
else {
vErrors.push(err210);
}
errors++;
}
}
}
}
var valid19 = _errs156 === errors;
}
else {
var valid19 = true;
}
if(valid19){
if(data56.stop_on_budget !== undefined){
const _errs159 = errors;
if(typeof data56.stop_on_budget !== "boolean"){
const err211 = {instancePath:instancePath+"/stop_rule/stop_on_budget",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_budget/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err211];
}
else {
vErrors.push(err211);
}
errors++;
}
var valid19 = _errs159 === errors;
}
else {
var valid19 = true;
}
if(valid19){
if(data56.stop_on_invalid_data !== undefined){
const _errs161 = errors;
if(typeof data56.stop_on_invalid_data !== "boolean"){
const err212 = {instancePath:instancePath+"/stop_rule/stop_on_invalid_data",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_invalid_data/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err212];
}
else {
vErrors.push(err212);
}
errors++;
}
var valid19 = _errs161 === errors;
}
else {
var valid19 = true;
}
if(valid19){
if(data56.stop_on_no_improvement_trials !== undefined){
let data60 = data56.stop_on_no_improvement_trials;
const _errs163 = errors;
if((!((typeof data60 == "number") && (!(data60 % 1) && !isNaN(data60)))) && (data60 !== null)){
const err213 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_no_improvement_trials/type",keyword:"type",params:{type: schema117.oneOf[1].properties.stop_rule.properties.stop_on_no_improvement_trials.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err213];
}
else {
vErrors.push(err213);
}
errors++;
}
if(errors === _errs163){
if(typeof data60 == "number"){
if(data60 > 65535 || isNaN(data60)){
const err214 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_no_improvement_trials/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err214];
}
else {
vErrors.push(err214);
}
errors++;
}
else {
if(data60 < 1 || isNaN(data60)){
const err215 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_no_improvement_trials/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err215];
}
else {
vErrors.push(err215);
}
errors++;
}
else {
if(!(formats94.validate(data60))){
const err216 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_no_improvement_trials/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err216];
}
else {
vErrors.push(err216);
}
errors++;
}
}
}
}
}
var valid19 = _errs163 === errors;
}
else {
var valid19 = true;
}
if(valid19){
if(data56.stop_on_qualified_count !== undefined){
let data61 = data56.stop_on_qualified_count;
const _errs165 = errors;
if(!((typeof data61 == "number") && (!(data61 % 1) && !isNaN(data61)))){
const err217 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_qualified_count/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err217];
}
else {
vErrors.push(err217);
}
errors++;
}
if(errors === _errs165){
if(typeof data61 == "number"){
if(data61 > 65535 || isNaN(data61)){
const err218 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_qualified_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err218];
}
else {
vErrors.push(err218);
}
errors++;
}
else {
if(data61 < 1 || isNaN(data61)){
const err219 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_qualified_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err219];
}
else {
vErrors.push(err219);
}
errors++;
}
else {
if(!(formats94.validate(data61))){
const err220 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/1/properties/stop_rule/properties/stop_on_qualified_count/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err220];
}
else {
vErrors.push(err220);
}
errors++;
}
}
}
}
}
var valid19 = _errs165 === errors;
}
else {
var valid19 = true;
}
}
}
}
}
}
}
}
else {
const err221 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/1/properties/stop_rule/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err221];
}
else {
vErrors.push(err221);
}
errors++;
}
}
var valid11 = _errs153 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.target_kind !== undefined){
let data62 = data.target_kind;
const _errs167 = errors;
if(typeof data62 !== "string"){
const err222 = {instancePath:instancePath+"/target_kind",schemaPath:"#/oneOf/1/properties/target_kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err222];
}
else {
vErrors.push(err222);
}
errors++;
}
if(!((data62 === "SCORE") || (data62 === "EXPECTED_RETURN"))){
const err223 = {instancePath:instancePath+"/target_kind",schemaPath:"#/oneOf/1/properties/target_kind/enum",keyword:"enum",params:{allowedValues: schema117.oneOf[1].properties.target_kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err223];
}
else {
vErrors.push(err223);
}
errors++;
}
var valid11 = _errs167 === errors;
}
else {
var valid11 = true;
}
if(valid11){
if(data.universe_version_id !== undefined){
let data63 = data.universe_version_id;
const _errs169 = errors;
if(errors === _errs169){
if(errors === _errs169){
if(typeof data63 === "string"){
if(!pattern5.test(data63)){
const err224 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/1/properties/universe_version_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err224];
}
else {
vErrors.push(err224);
}
errors++;
}
else {
if(!(formats2.test(data63))){
const err225 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/1/properties/universe_version_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err225];
}
else {
vErrors.push(err225);
}
errors++;
}
}
}
else {
const err226 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/1/properties/universe_version_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err226];
}
else {
vErrors.push(err226);
}
errors++;
}
}
}
var valid11 = _errs169 === errors;
}
else {
var valid11 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
const err227 = {instancePath,schemaPath:"#/oneOf/1/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err227];
}
else {
vErrors.push(err227);
}
errors++;
}
}
var _valid0 = _errs86 === errors;
if(_valid0 && valid0){
valid0 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid0 = true;
passing0 = 1;
if(props0 !== true){
props0 = true;
}
}
const _errs171 = errors;
if(errors === _errs171){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing6;
if(((((((((((data.hypothesis === undefined) && (missing6 = "hypothesis")) || ((data.economic_rationale === undefined) && (missing6 = "economic_rationale"))) || ((data.universe_version_id === undefined) && (missing6 = "universe_version_id"))) || ((data.target_kind === undefined) && (missing6 = "target_kind"))) || ((data.horizon_kind === undefined) && (missing6 = "horizon_kind"))) || ((data.base_currency === undefined) && (missing6 = "base_currency"))) || ((data.evaluation_policy_id === undefined) && (missing6 = "evaluation_policy_id"))) || ((data.execution_assumptions_id === undefined) && (missing6 = "execution_assumptions_id"))) || ((data.budget === undefined) && (missing6 = "budget"))) || ((data.stop_rule === undefined) && (missing6 = "stop_rule"))){
const err228 = {instancePath,schemaPath:"#/oneOf/2/required",keyword:"required",params:{missingProperty: missing6},message:"must have required property '"+missing6+"'"};
if(vErrors === null){
vErrors = [err228];
}
else {
vErrors.push(err228);
}
errors++;
}
else {
const _errs173 = errors;
for(const key6 in data){
if(!(func1.call(schema117.oneOf[2].properties, key6))){
const err229 = {instancePath,schemaPath:"#/oneOf/2/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key6},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err229];
}
else {
vErrors.push(err229);
}
errors++;
break;
}
}
if(_errs173 === errors){
if(data.base_currency !== undefined){
let data64 = data.base_currency;
const _errs174 = errors;
if(errors === _errs174){
if(typeof data64 === "string"){
if(func2(data64) > 3){
const err230 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/2/properties/base_currency/maxLength",keyword:"maxLength",params:{limit: 3},message:"must NOT have more than 3 characters"};
if(vErrors === null){
vErrors = [err230];
}
else {
vErrors.push(err230);
}
errors++;
}
else {
if(func2(data64) < 3){
const err231 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/2/properties/base_currency/minLength",keyword:"minLength",params:{limit: 3},message:"must NOT have fewer than 3 characters"};
if(vErrors === null){
vErrors = [err231];
}
else {
vErrors.push(err231);
}
errors++;
}
else {
if(!pattern36.test(data64)){
const err232 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/2/properties/base_currency/pattern",keyword:"pattern",params:{pattern: "^[A-Z]{3}$"},message:"must match pattern \""+"^[A-Z]{3}$"+"\""};
if(vErrors === null){
vErrors = [err232];
}
else {
vErrors.push(err232);
}
errors++;
}
}
}
}
else {
const err233 = {instancePath:instancePath+"/base_currency",schemaPath:"#/oneOf/2/properties/base_currency/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err233];
}
else {
vErrors.push(err233);
}
errors++;
}
}
var valid21 = _errs174 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.benchmark_ref !== undefined){
let data65 = data.benchmark_ref;
const _errs176 = errors;
const _errs177 = errors;
let valid22 = false;
let passing7 = null;
const _errs178 = errors;
if(data65 !== null){
const err234 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/2/properties/benchmark_ref/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err234];
}
else {
vErrors.push(err234);
}
errors++;
}
var _valid7 = _errs178 === errors;
if(_valid7){
valid22 = true;
passing7 = 0;
}
const _errs180 = errors;
if(errors === _errs180){
if(errors === _errs180){
if(typeof data65 === "string"){
if(!pattern5.test(data65)){
const err235 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/2/properties/benchmark_ref/oneOf/1/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err235];
}
else {
vErrors.push(err235);
}
errors++;
}
else {
if(!(formats2.test(data65))){
const err236 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/2/properties/benchmark_ref/oneOf/1/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err236];
}
else {
vErrors.push(err236);
}
errors++;
}
}
}
else {
const err237 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/2/properties/benchmark_ref/oneOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err237];
}
else {
vErrors.push(err237);
}
errors++;
}
}
}
var _valid7 = _errs180 === errors;
if(_valid7 && valid22){
valid22 = false;
passing7 = [passing7, 1];
}
else {
if(_valid7){
valid22 = true;
passing7 = 1;
}
}
if(!valid22){
const err238 = {instancePath:instancePath+"/benchmark_ref",schemaPath:"#/oneOf/2/properties/benchmark_ref/oneOf",keyword:"oneOf",params:{passingSchemas: passing7},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err238];
}
else {
vErrors.push(err238);
}
errors++;
}
else {
errors = _errs177;
if(vErrors !== null){
if(_errs177){
vErrors.length = _errs177;
}
else {
vErrors = null;
}
}
}
var valid21 = _errs176 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.budget !== undefined){
let data66 = data.budget;
const _errs182 = errors;
if(errors === _errs182){
if(data66 && typeof data66 == "object" && !Array.isArray(data66)){
let missing7;
if(((((((((((((data66.schema_version === undefined) && (missing7 = "schema_version")) || ((data66.max_experiments === undefined) && (missing7 = "max_experiments"))) || ((data66.max_parallel_runs === undefined) && (missing7 = "max_parallel_runs"))) || ((data66.max_turns_per_mission === undefined) && (missing7 = "max_turns_per_mission"))) || ((data66.max_repair_turns === undefined) && (missing7 = "max_repair_turns"))) || ((data66.max_wall_seconds === undefined) && (missing7 = "max_wall_seconds"))) || ((data66.max_cpu_seconds === undefined) && (missing7 = "max_cpu_seconds"))) || ((data66.max_memory_mib === undefined) && (missing7 = "max_memory_mib"))) || ((data66.max_output_bytes === undefined) && (missing7 = "max_output_bytes"))) || ((data66.max_cycles_per_day === undefined) && (missing7 = "max_cycles_per_day"))) || ((data66.min_cycle_interval_seconds === undefined) && (missing7 = "min_cycle_interval_seconds"))) || ((data66.cost_enforcement === undefined) && (missing7 = "cost_enforcement"))){
const err239 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/2/properties/budget/required",keyword:"required",params:{missingProperty: missing7},message:"must have required property '"+missing7+"'"};
if(vErrors === null){
vErrors = [err239];
}
else {
vErrors.push(err239);
}
errors++;
}
else {
const _errs184 = errors;
for(const key7 in data66){
if(!(func1.call(schema117.oneOf[2].properties.budget.properties, key7))){
const err240 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/2/properties/budget/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key7},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err240];
}
else {
vErrors.push(err240);
}
errors++;
break;
}
}
if(_errs184 === errors){
if(data66.cost_currency !== undefined){
let data67 = data66.cost_currency;
const _errs185 = errors;
if((typeof data67 !== "string") && (data67 !== null)){
const err241 = {instancePath:instancePath+"/budget/cost_currency",schemaPath:"#/oneOf/2/properties/budget/properties/cost_currency/type",keyword:"type",params:{type: schema117.oneOf[2].properties.budget.properties.cost_currency.type},message:"must be string,null"};
if(vErrors === null){
vErrors = [err241];
}
else {
vErrors.push(err241);
}
errors++;
}
var valid23 = _errs185 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.cost_enforcement !== undefined){
let data68 = data66.cost_enforcement;
const _errs187 = errors;
if(typeof data68 !== "string"){
const err242 = {instancePath:instancePath+"/budget/cost_enforcement",schemaPath:"#/components/schemas/CostEnforcement/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err242];
}
else {
vErrors.push(err242);
}
errors++;
}
if(!(((data68 === "UNAVAILABLE") || (data68 === "ESTIMATED")) || (data68 === "EXACT"))){
const err243 = {instancePath:instancePath+"/budget/cost_enforcement",schemaPath:"#/components/schemas/CostEnforcement/enum",keyword:"enum",params:{allowedValues: schema118.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err243];
}
else {
vErrors.push(err243);
}
errors++;
}
var valid23 = _errs187 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_cost_decimal !== undefined){
let data69 = data66.max_cost_decimal;
const _errs190 = errors;
const _errs191 = errors;
let valid25 = false;
let passing8 = null;
const _errs192 = errors;
if(data69 !== null){
const err244 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/oneOf/2/properties/budget/properties/max_cost_decimal/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err244];
}
else {
vErrors.push(err244);
}
errors++;
}
var _valid8 = _errs192 === errors;
if(_valid8){
valid25 = true;
passing8 = 0;
}
const _errs194 = errors;
const _errs195 = errors;
if(errors === _errs195){
if(typeof data69 === "string"){
if(func2(data69) > 64){
const err245 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"};
if(vErrors === null){
vErrors = [err245];
}
else {
vErrors.push(err245);
}
errors++;
}
else {
if(func2(data69) < 1){
const err246 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err246];
}
else {
vErrors.push(err246);
}
errors++;
}
else {
if(!pattern38.test(data69)){
const err247 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err247];
}
else {
vErrors.push(err247);
}
errors++;
}
}
}
}
else {
const err248 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/components/schemas/DecimalValue/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err248];
}
else {
vErrors.push(err248);
}
errors++;
}
}
var _valid8 = _errs194 === errors;
if(_valid8 && valid25){
valid25 = false;
passing8 = [passing8, 1];
}
else {
if(_valid8){
valid25 = true;
passing8 = 1;
}
}
if(!valid25){
const err249 = {instancePath:instancePath+"/budget/max_cost_decimal",schemaPath:"#/oneOf/2/properties/budget/properties/max_cost_decimal/oneOf",keyword:"oneOf",params:{passingSchemas: passing8},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err249];
}
else {
vErrors.push(err249);
}
errors++;
}
else {
errors = _errs191;
if(vErrors !== null){
if(_errs191){
vErrors.length = _errs191;
}
else {
vErrors = null;
}
}
}
var valid23 = _errs190 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_cpu_seconds !== undefined){
let data70 = data66.max_cpu_seconds;
const _errs197 = errors;
if(errors === _errs197){
if(typeof data70 === "string"){
if(func2(data70) > 19){
const err250 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_cpu_seconds/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err250];
}
else {
vErrors.push(err250);
}
errors++;
}
else {
if(func2(data70) < 1){
const err251 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_cpu_seconds/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err251];
}
else {
vErrors.push(err251);
}
errors++;
}
else {
if(!pattern27.test(data70)){
const err252 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_cpu_seconds/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err252];
}
else {
vErrors.push(err252);
}
errors++;
}
}
}
}
else {
const err253 = {instancePath:instancePath+"/budget/max_cpu_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_cpu_seconds/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err253];
}
else {
vErrors.push(err253);
}
errors++;
}
}
var valid23 = _errs197 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_cycles_per_day !== undefined){
let data71 = data66.max_cycles_per_day;
const _errs199 = errors;
if(!((typeof data71 == "number") && (!(data71 % 1) && !isNaN(data71)))){
const err254 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/2/properties/budget/properties/max_cycles_per_day/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err254];
}
else {
vErrors.push(err254);
}
errors++;
}
if(errors === _errs199){
if(typeof data71 == "number"){
if(data71 > 65535 || isNaN(data71)){
const err255 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/2/properties/budget/properties/max_cycles_per_day/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err255];
}
else {
vErrors.push(err255);
}
errors++;
}
else {
if(data71 < 1 || isNaN(data71)){
const err256 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/2/properties/budget/properties/max_cycles_per_day/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err256];
}
else {
vErrors.push(err256);
}
errors++;
}
else {
if(!(formats94.validate(data71))){
const err257 = {instancePath:instancePath+"/budget/max_cycles_per_day",schemaPath:"#/oneOf/2/properties/budget/properties/max_cycles_per_day/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err257];
}
else {
vErrors.push(err257);
}
errors++;
}
}
}
}
}
var valid23 = _errs199 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_experiments !== undefined){
let data72 = data66.max_experiments;
const _errs201 = errors;
if(!((typeof data72 == "number") && (!(data72 % 1) && !isNaN(data72)))){
const err258 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/2/properties/budget/properties/max_experiments/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err258];
}
else {
vErrors.push(err258);
}
errors++;
}
if(errors === _errs201){
if(typeof data72 == "number"){
if(data72 > 4294967295 || isNaN(data72)){
const err259 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/2/properties/budget/properties/max_experiments/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err259];
}
else {
vErrors.push(err259);
}
errors++;
}
else {
if(data72 < 1 || isNaN(data72)){
const err260 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/2/properties/budget/properties/max_experiments/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err260];
}
else {
vErrors.push(err260);
}
errors++;
}
else {
if(!(formats96.validate(data72))){
const err261 = {instancePath:instancePath+"/budget/max_experiments",schemaPath:"#/oneOf/2/properties/budget/properties/max_experiments/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err261];
}
else {
vErrors.push(err261);
}
errors++;
}
}
}
}
}
var valid23 = _errs201 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_memory_mib !== undefined){
let data73 = data66.max_memory_mib;
const _errs203 = errors;
if(!((typeof data73 == "number") && (!(data73 % 1) && !isNaN(data73)))){
const err262 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/2/properties/budget/properties/max_memory_mib/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err262];
}
else {
vErrors.push(err262);
}
errors++;
}
if(errors === _errs203){
if(typeof data73 == "number"){
if(data73 > 4294967295 || isNaN(data73)){
const err263 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/2/properties/budget/properties/max_memory_mib/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err263];
}
else {
vErrors.push(err263);
}
errors++;
}
else {
if(data73 < 1 || isNaN(data73)){
const err264 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/2/properties/budget/properties/max_memory_mib/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err264];
}
else {
vErrors.push(err264);
}
errors++;
}
else {
if(!(formats96.validate(data73))){
const err265 = {instancePath:instancePath+"/budget/max_memory_mib",schemaPath:"#/oneOf/2/properties/budget/properties/max_memory_mib/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err265];
}
else {
vErrors.push(err265);
}
errors++;
}
}
}
}
}
var valid23 = _errs203 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_output_bytes !== undefined){
let data74 = data66.max_output_bytes;
const _errs205 = errors;
if(errors === _errs205){
if(typeof data74 === "string"){
if(func2(data74) > 19){
const err266 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/2/properties/budget/properties/max_output_bytes/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err266];
}
else {
vErrors.push(err266);
}
errors++;
}
else {
if(func2(data74) < 1){
const err267 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/2/properties/budget/properties/max_output_bytes/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err267];
}
else {
vErrors.push(err267);
}
errors++;
}
else {
if(!pattern27.test(data74)){
const err268 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/2/properties/budget/properties/max_output_bytes/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err268];
}
else {
vErrors.push(err268);
}
errors++;
}
}
}
}
else {
const err269 = {instancePath:instancePath+"/budget/max_output_bytes",schemaPath:"#/oneOf/2/properties/budget/properties/max_output_bytes/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err269];
}
else {
vErrors.push(err269);
}
errors++;
}
}
var valid23 = _errs205 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_parallel_runs !== undefined){
let data75 = data66.max_parallel_runs;
const _errs207 = errors;
if(!((typeof data75 == "number") && (!(data75 % 1) && !isNaN(data75)))){
const err270 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/2/properties/budget/properties/max_parallel_runs/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err270];
}
else {
vErrors.push(err270);
}
errors++;
}
if(errors === _errs207){
if(typeof data75 == "number"){
if(data75 > 65535 || isNaN(data75)){
const err271 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/2/properties/budget/properties/max_parallel_runs/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err271];
}
else {
vErrors.push(err271);
}
errors++;
}
else {
if(data75 < 1 || isNaN(data75)){
const err272 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/2/properties/budget/properties/max_parallel_runs/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err272];
}
else {
vErrors.push(err272);
}
errors++;
}
else {
if(!(formats94.validate(data75))){
const err273 = {instancePath:instancePath+"/budget/max_parallel_runs",schemaPath:"#/oneOf/2/properties/budget/properties/max_parallel_runs/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err273];
}
else {
vErrors.push(err273);
}
errors++;
}
}
}
}
}
var valid23 = _errs207 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_repair_turns !== undefined){
let data76 = data66.max_repair_turns;
const _errs209 = errors;
if(!((typeof data76 == "number") && (!(data76 % 1) && !isNaN(data76)))){
const err274 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/2/properties/budget/properties/max_repair_turns/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err274];
}
else {
vErrors.push(err274);
}
errors++;
}
if(errors === _errs209){
if(typeof data76 == "number"){
if(data76 > 65535 || isNaN(data76)){
const err275 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/2/properties/budget/properties/max_repair_turns/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err275];
}
else {
vErrors.push(err275);
}
errors++;
}
else {
if(data76 < 0 || isNaN(data76)){
const err276 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/2/properties/budget/properties/max_repair_turns/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err276];
}
else {
vErrors.push(err276);
}
errors++;
}
else {
if(!(formats94.validate(data76))){
const err277 = {instancePath:instancePath+"/budget/max_repair_turns",schemaPath:"#/oneOf/2/properties/budget/properties/max_repair_turns/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err277];
}
else {
vErrors.push(err277);
}
errors++;
}
}
}
}
}
var valid23 = _errs209 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_tokens !== undefined){
let data77 = data66.max_tokens;
const _errs211 = errors;
const _errs212 = errors;
let valid27 = false;
let passing9 = null;
const _errs213 = errors;
if(data77 !== null){
const err278 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/2/properties/budget/properties/max_tokens/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err278];
}
else {
vErrors.push(err278);
}
errors++;
}
var _valid9 = _errs213 === errors;
if(_valid9){
valid27 = true;
passing9 = 0;
}
const _errs215 = errors;
if(errors === _errs215){
if(typeof data77 === "string"){
if(func2(data77) > 19){
const err279 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/2/properties/budget/properties/max_tokens/oneOf/1/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err279];
}
else {
vErrors.push(err279);
}
errors++;
}
else {
if(func2(data77) < 1){
const err280 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/2/properties/budget/properties/max_tokens/oneOf/1/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err280];
}
else {
vErrors.push(err280);
}
errors++;
}
else {
if(!pattern27.test(data77)){
const err281 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/2/properties/budget/properties/max_tokens/oneOf/1/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err281];
}
else {
vErrors.push(err281);
}
errors++;
}
}
}
}
else {
const err282 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/2/properties/budget/properties/max_tokens/oneOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err282];
}
else {
vErrors.push(err282);
}
errors++;
}
}
var _valid9 = _errs215 === errors;
if(_valid9 && valid27){
valid27 = false;
passing9 = [passing9, 1];
}
else {
if(_valid9){
valid27 = true;
passing9 = 1;
}
}
if(!valid27){
const err283 = {instancePath:instancePath+"/budget/max_tokens",schemaPath:"#/oneOf/2/properties/budget/properties/max_tokens/oneOf",keyword:"oneOf",params:{passingSchemas: passing9},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err283];
}
else {
vErrors.push(err283);
}
errors++;
}
else {
errors = _errs212;
if(vErrors !== null){
if(_errs212){
vErrors.length = _errs212;
}
else {
vErrors = null;
}
}
}
var valid23 = _errs211 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_turns_per_mission !== undefined){
let data78 = data66.max_turns_per_mission;
const _errs217 = errors;
if(!((typeof data78 == "number") && (!(data78 % 1) && !isNaN(data78)))){
const err284 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/2/properties/budget/properties/max_turns_per_mission/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err284];
}
else {
vErrors.push(err284);
}
errors++;
}
if(errors === _errs217){
if(typeof data78 == "number"){
if(data78 > 65535 || isNaN(data78)){
const err285 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/2/properties/budget/properties/max_turns_per_mission/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err285];
}
else {
vErrors.push(err285);
}
errors++;
}
else {
if(data78 < 1 || isNaN(data78)){
const err286 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/2/properties/budget/properties/max_turns_per_mission/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err286];
}
else {
vErrors.push(err286);
}
errors++;
}
else {
if(!(formats94.validate(data78))){
const err287 = {instancePath:instancePath+"/budget/max_turns_per_mission",schemaPath:"#/oneOf/2/properties/budget/properties/max_turns_per_mission/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err287];
}
else {
vErrors.push(err287);
}
errors++;
}
}
}
}
}
var valid23 = _errs217 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.max_wall_seconds !== undefined){
let data79 = data66.max_wall_seconds;
const _errs219 = errors;
if(!((typeof data79 == "number") && (!(data79 % 1) && !isNaN(data79)))){
const err288 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_wall_seconds/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err288];
}
else {
vErrors.push(err288);
}
errors++;
}
if(errors === _errs219){
if(typeof data79 == "number"){
if(data79 > 4294967295 || isNaN(data79)){
const err289 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_wall_seconds/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err289];
}
else {
vErrors.push(err289);
}
errors++;
}
else {
if(data79 < 1 || isNaN(data79)){
const err290 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_wall_seconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err290];
}
else {
vErrors.push(err290);
}
errors++;
}
else {
if(!(formats96.validate(data79))){
const err291 = {instancePath:instancePath+"/budget/max_wall_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/max_wall_seconds/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err291];
}
else {
vErrors.push(err291);
}
errors++;
}
}
}
}
}
var valid23 = _errs219 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.min_cycle_interval_seconds !== undefined){
let data80 = data66.min_cycle_interval_seconds;
const _errs221 = errors;
if(!((typeof data80 == "number") && (!(data80 % 1) && !isNaN(data80)))){
const err292 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/min_cycle_interval_seconds/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err292];
}
else {
vErrors.push(err292);
}
errors++;
}
if(errors === _errs221){
if(typeof data80 == "number"){
if(data80 > 4294967295 || isNaN(data80)){
const err293 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/min_cycle_interval_seconds/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"};
if(vErrors === null){
vErrors = [err293];
}
else {
vErrors.push(err293);
}
errors++;
}
else {
if(data80 < 0 || isNaN(data80)){
const err294 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/min_cycle_interval_seconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err294];
}
else {
vErrors.push(err294);
}
errors++;
}
else {
if(!(formats96.validate(data80))){
const err295 = {instancePath:instancePath+"/budget/min_cycle_interval_seconds",schemaPath:"#/oneOf/2/properties/budget/properties/min_cycle_interval_seconds/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""};
if(vErrors === null){
vErrors = [err295];
}
else {
vErrors.push(err295);
}
errors++;
}
}
}
}
}
var valid23 = _errs221 === errors;
}
else {
var valid23 = true;
}
if(valid23){
if(data66.schema_version !== undefined){
let data81 = data66.schema_version;
const _errs223 = errors;
const _errs224 = errors;
if(!((typeof data81 == "number") && (!(data81 % 1) && !isNaN(data81)))){
const err296 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err296];
}
else {
vErrors.push(err296);
}
errors++;
}
if(!(data81 === 1)){
const err297 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err297];
}
else {
vErrors.push(err297);
}
errors++;
}
if(errors === _errs224){
if(typeof data81 == "number"){
if(data81 > 1 || isNaN(data81)){
const err298 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"};
if(vErrors === null){
vErrors = [err298];
}
else {
vErrors.push(err298);
}
errors++;
}
else {
if(data81 < 1 || isNaN(data81)){
const err299 = {instancePath:instancePath+"/budget/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err299];
}
else {
vErrors.push(err299);
}
errors++;
}
}
}
}
var valid23 = _errs223 === errors;
}
else {
var valid23 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
const err300 = {instancePath:instancePath+"/budget",schemaPath:"#/oneOf/2/properties/budget/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err300];
}
else {
vErrors.push(err300);
}
errors++;
}
}
var valid21 = _errs182 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.economic_rationale !== undefined){
let data82 = data.economic_rationale;
const _errs226 = errors;
if(errors === _errs226){
if(typeof data82 === "string"){
if(func2(data82) > 8000){
const err301 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/2/properties/economic_rationale/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"};
if(vErrors === null){
vErrors = [err301];
}
else {
vErrors.push(err301);
}
errors++;
}
else {
if(func2(data82) < 1){
const err302 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/2/properties/economic_rationale/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err302];
}
else {
vErrors.push(err302);
}
errors++;
}
}
}
else {
const err303 = {instancePath:instancePath+"/economic_rationale",schemaPath:"#/oneOf/2/properties/economic_rationale/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err303];
}
else {
vErrors.push(err303);
}
errors++;
}
}
var valid21 = _errs226 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.evaluation_policy_id !== undefined){
let data83 = data.evaluation_policy_id;
const _errs228 = errors;
if(errors === _errs228){
if(errors === _errs228){
if(typeof data83 === "string"){
if(!pattern5.test(data83)){
const err304 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/2/properties/evaluation_policy_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err304];
}
else {
vErrors.push(err304);
}
errors++;
}
else {
if(!(formats2.test(data83))){
const err305 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/2/properties/evaluation_policy_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err305];
}
else {
vErrors.push(err305);
}
errors++;
}
}
}
else {
const err306 = {instancePath:instancePath+"/evaluation_policy_id",schemaPath:"#/oneOf/2/properties/evaluation_policy_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err306];
}
else {
vErrors.push(err306);
}
errors++;
}
}
}
var valid21 = _errs228 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.execution_assumptions_id !== undefined){
let data84 = data.execution_assumptions_id;
const _errs230 = errors;
if(errors === _errs230){
if(errors === _errs230){
if(typeof data84 === "string"){
if(!pattern5.test(data84)){
const err307 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/2/properties/execution_assumptions_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err307];
}
else {
vErrors.push(err307);
}
errors++;
}
else {
if(!(formats2.test(data84))){
const err308 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/2/properties/execution_assumptions_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err308];
}
else {
vErrors.push(err308);
}
errors++;
}
}
}
else {
const err309 = {instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/oneOf/2/properties/execution_assumptions_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err309];
}
else {
vErrors.push(err309);
}
errors++;
}
}
}
var valid21 = _errs230 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.horizon_kind !== undefined){
let data85 = data.horizon_kind;
const _errs232 = errors;
if(typeof data85 !== "string"){
const err310 = {instancePath:instancePath+"/horizon_kind",schemaPath:"#/oneOf/2/properties/horizon_kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err310];
}
else {
vErrors.push(err310);
}
errors++;
}
if(!(data85 === "VARIABLE_INTERVAL")){
const err311 = {instancePath:instancePath+"/horizon_kind",schemaPath:"#/oneOf/2/properties/horizon_kind/enum",keyword:"enum",params:{allowedValues: schema117.oneOf[2].properties.horizon_kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err311];
}
else {
vErrors.push(err311);
}
errors++;
}
var valid21 = _errs232 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.horizon_value !== undefined){
const _errs234 = errors;
if(data.horizon_value !== null){
const err312 = {instancePath:instancePath+"/horizon_value",schemaPath:"#/oneOf/2/properties/horizon_value/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err312];
}
else {
vErrors.push(err312);
}
errors++;
}
var valid21 = _errs234 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.hypothesis !== undefined){
let data87 = data.hypothesis;
const _errs236 = errors;
if(errors === _errs236){
if(typeof data87 === "string"){
if(func2(data87) > 8000){
const err313 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/2/properties/hypothesis/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"};
if(vErrors === null){
vErrors = [err313];
}
else {
vErrors.push(err313);
}
errors++;
}
else {
if(func2(data87) < 1){
const err314 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/2/properties/hypothesis/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err314];
}
else {
vErrors.push(err314);
}
errors++;
}
}
}
else {
const err315 = {instancePath:instancePath+"/hypothesis",schemaPath:"#/oneOf/2/properties/hypothesis/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err315];
}
else {
vErrors.push(err315);
}
errors++;
}
}
var valid21 = _errs236 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.stop_rule !== undefined){
let data88 = data.stop_rule;
const _errs238 = errors;
if(errors === _errs238){
if(data88 && typeof data88 == "object" && !Array.isArray(data88)){
let missing8;
if(((((data88.schema_version === undefined) && (missing8 = "schema_version")) || ((data88.stop_on_qualified_count === undefined) && (missing8 = "stop_on_qualified_count"))) || ((data88.stop_on_budget === undefined) && (missing8 = "stop_on_budget"))) || ((data88.stop_on_invalid_data === undefined) && (missing8 = "stop_on_invalid_data"))){
const err316 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/2/properties/stop_rule/required",keyword:"required",params:{missingProperty: missing8},message:"must have required property '"+missing8+"'"};
if(vErrors === null){
vErrors = [err316];
}
else {
vErrors.push(err316);
}
errors++;
}
else {
const _errs240 = errors;
for(const key8 in data88){
if(!(((((key8 === "schema_version") || (key8 === "stop_on_budget")) || (key8 === "stop_on_invalid_data")) || (key8 === "stop_on_no_improvement_trials")) || (key8 === "stop_on_qualified_count"))){
const err317 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/2/properties/stop_rule/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key8},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err317];
}
else {
vErrors.push(err317);
}
errors++;
break;
}
}
if(_errs240 === errors){
if(data88.schema_version !== undefined){
let data89 = data88.schema_version;
const _errs241 = errors;
const _errs242 = errors;
if(!((typeof data89 == "number") && (!(data89 % 1) && !isNaN(data89)))){
const err318 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err318];
}
else {
vErrors.push(err318);
}
errors++;
}
if(!(data89 === 1)){
const err319 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err319];
}
else {
vErrors.push(err319);
}
errors++;
}
if(errors === _errs242){
if(typeof data89 == "number"){
if(data89 > 1 || isNaN(data89)){
const err320 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"};
if(vErrors === null){
vErrors = [err320];
}
else {
vErrors.push(err320);
}
errors++;
}
else {
if(data89 < 1 || isNaN(data89)){
const err321 = {instancePath:instancePath+"/stop_rule/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err321];
}
else {
vErrors.push(err321);
}
errors++;
}
}
}
}
var valid29 = _errs241 === errors;
}
else {
var valid29 = true;
}
if(valid29){
if(data88.stop_on_budget !== undefined){
const _errs244 = errors;
if(typeof data88.stop_on_budget !== "boolean"){
const err322 = {instancePath:instancePath+"/stop_rule/stop_on_budget",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_budget/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err322];
}
else {
vErrors.push(err322);
}
errors++;
}
var valid29 = _errs244 === errors;
}
else {
var valid29 = true;
}
if(valid29){
if(data88.stop_on_invalid_data !== undefined){
const _errs246 = errors;
if(typeof data88.stop_on_invalid_data !== "boolean"){
const err323 = {instancePath:instancePath+"/stop_rule/stop_on_invalid_data",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_invalid_data/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err323];
}
else {
vErrors.push(err323);
}
errors++;
}
var valid29 = _errs246 === errors;
}
else {
var valid29 = true;
}
if(valid29){
if(data88.stop_on_no_improvement_trials !== undefined){
let data92 = data88.stop_on_no_improvement_trials;
const _errs248 = errors;
if((!((typeof data92 == "number") && (!(data92 % 1) && !isNaN(data92)))) && (data92 !== null)){
const err324 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_no_improvement_trials/type",keyword:"type",params:{type: schema117.oneOf[2].properties.stop_rule.properties.stop_on_no_improvement_trials.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err324];
}
else {
vErrors.push(err324);
}
errors++;
}
if(errors === _errs248){
if(typeof data92 == "number"){
if(data92 > 65535 || isNaN(data92)){
const err325 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_no_improvement_trials/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err325];
}
else {
vErrors.push(err325);
}
errors++;
}
else {
if(data92 < 1 || isNaN(data92)){
const err326 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_no_improvement_trials/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err326];
}
else {
vErrors.push(err326);
}
errors++;
}
else {
if(!(formats94.validate(data92))){
const err327 = {instancePath:instancePath+"/stop_rule/stop_on_no_improvement_trials",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_no_improvement_trials/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err327];
}
else {
vErrors.push(err327);
}
errors++;
}
}
}
}
}
var valid29 = _errs248 === errors;
}
else {
var valid29 = true;
}
if(valid29){
if(data88.stop_on_qualified_count !== undefined){
let data93 = data88.stop_on_qualified_count;
const _errs250 = errors;
if(!((typeof data93 == "number") && (!(data93 % 1) && !isNaN(data93)))){
const err328 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_qualified_count/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err328];
}
else {
vErrors.push(err328);
}
errors++;
}
if(errors === _errs250){
if(typeof data93 == "number"){
if(data93 > 65535 || isNaN(data93)){
const err329 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_qualified_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err329];
}
else {
vErrors.push(err329);
}
errors++;
}
else {
if(data93 < 1 || isNaN(data93)){
const err330 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_qualified_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"};
if(vErrors === null){
vErrors = [err330];
}
else {
vErrors.push(err330);
}
errors++;
}
else {
if(!(formats94.validate(data93))){
const err331 = {instancePath:instancePath+"/stop_rule/stop_on_qualified_count",schemaPath:"#/oneOf/2/properties/stop_rule/properties/stop_on_qualified_count/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""};
if(vErrors === null){
vErrors = [err331];
}
else {
vErrors.push(err331);
}
errors++;
}
}
}
}
}
var valid29 = _errs250 === errors;
}
else {
var valid29 = true;
}
}
}
}
}
}
}
}
else {
const err332 = {instancePath:instancePath+"/stop_rule",schemaPath:"#/oneOf/2/properties/stop_rule/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err332];
}
else {
vErrors.push(err332);
}
errors++;
}
}
var valid21 = _errs238 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.target_kind !== undefined){
let data94 = data.target_kind;
const _errs252 = errors;
if(typeof data94 !== "string"){
const err333 = {instancePath:instancePath+"/target_kind",schemaPath:"#/oneOf/2/properties/target_kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err333];
}
else {
vErrors.push(err333);
}
errors++;
}
if(!((data94 === "SCORE") || (data94 === "EXPECTED_RETURN"))){
const err334 = {instancePath:instancePath+"/target_kind",schemaPath:"#/oneOf/2/properties/target_kind/enum",keyword:"enum",params:{allowedValues: schema117.oneOf[2].properties.target_kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err334];
}
else {
vErrors.push(err334);
}
errors++;
}
var valid21 = _errs252 === errors;
}
else {
var valid21 = true;
}
if(valid21){
if(data.universe_version_id !== undefined){
let data95 = data.universe_version_id;
const _errs254 = errors;
if(errors === _errs254){
if(errors === _errs254){
if(typeof data95 === "string"){
if(!pattern5.test(data95)){
const err335 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/2/properties/universe_version_id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err335];
}
else {
vErrors.push(err335);
}
errors++;
}
else {
if(!(formats2.test(data95))){
const err336 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/2/properties/universe_version_id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err336];
}
else {
vErrors.push(err336);
}
errors++;
}
}
}
else {
const err337 = {instancePath:instancePath+"/universe_version_id",schemaPath:"#/oneOf/2/properties/universe_version_id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err337];
}
else {
vErrors.push(err337);
}
errors++;
}
}
}
var valid21 = _errs254 === errors;
}
else {
var valid21 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
const err338 = {instancePath,schemaPath:"#/oneOf/2/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err338];
}
else {
vErrors.push(err338);
}
errors++;
}
}
var _valid0 = _errs171 === errors;
if(_valid0 && valid0){
valid0 = false;
passing0 = [passing0, 2];
}
else {
if(_valid0){
valid0 = true;
passing0 = 2;
if(props0 !== true){
props0 = true;
}
}
}
}
if(!valid0){
const err339 = {instancePath,schemaPath:"#/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err339];
}
else {
vErrors.push(err339);
}
errors++;
validate63.errors = vErrors;
return false;
}
else {
errors = _errs0;
if(vErrors !== null){
if(_errs0){
vErrors.length = _errs0;
}
else {
vErrors = null;
}
}
}
validate63.errors = vErrors;
evaluated0.props = props0;
return errors === 0;
}
validate63.evaluated = {"dynamicProps":true,"dynamicItems":false};


function validate60(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate60.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((((data.id === undefined) && (missing0 = "id")) || ((data.project_id === undefined) && (missing0 = "project_id"))) || ((data.version === undefined) && (missing0 = "version"))) || ((data.revision === undefined) && (missing0 = "revision"))) || ((data.state === undefined) && (missing0 = "state"))) || ((data.content === undefined) && (missing0 = "content"))) || ((data.bindings === undefined) && (missing0 = "bindings"))) || ((data.created_at === undefined) && (missing0 = "created_at"))) || ((data.updated_at === undefined) && (missing0 = "updated_at"))){
validate60.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema112.properties, key0))){
validate60.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.bindings !== undefined){
let data0 = data.bindings;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
if(data0.length > 64){
validate60.errors = [{instancePath:instancePath+"/bindings",schemaPath:"#/properties/bindings/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data0.length < 1){
validate60.errors = [{instancePath:instancePath+"/bindings",schemaPath:"#/properties/bindings/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs4 = errors;
if(!(validate61(data0[i0], {instancePath:instancePath+"/bindings/" + i0,parentData:data0,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate61.errors : vErrors.concat(validate61.errors);
errors = vErrors.length;
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
}
}
else {
validate60.errors = [{instancePath:instancePath+"/bindings",schemaPath:"#/properties/bindings/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.content !== undefined){
const _errs5 = errors;
if(!(validate63(data.content, {instancePath:instancePath+"/content",parentData:data,parentDataProperty:"content",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate63.errors : vErrors.concat(validate63.errors);
errors = vErrors.length;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_at !== undefined){
let data3 = data.created_at;
const _errs6 = errors;
if(errors === _errs6){
if(errors === _errs6){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate60.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate60.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.frozen_at !== undefined){
let data4 = data.frozen_at;
const _errs8 = errors;
if((typeof data4 !== "string") && (data4 !== null)){
validate60.errors = [{instancePath:instancePath+"/frozen_at",schemaPath:"#/properties/frozen_at/type",keyword:"type",params:{type: schema112.properties.frozen_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs8){
if(errors === _errs8){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate60.errors = [{instancePath:instancePath+"/frozen_at",schemaPath:"#/properties/frozen_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data5 = data.id;
const _errs10 = errors;
const _errs11 = errors;
if(errors === _errs11){
if(errors === _errs11){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
validate60.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data5))){
validate60.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate60.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs10 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.project_id !== undefined){
let data6 = data.project_id;
const _errs13 = errors;
const _errs14 = errors;
if(errors === _errs14){
if(errors === _errs14){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate60.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate60.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate60.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revision !== undefined){
let data7 = data.revision;
const _errs16 = errors;
const _errs17 = errors;
if(errors === _errs17){
if(typeof data7 === "string"){
if(func2(data7) > 19){
validate60.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data7) < 1){
validate60.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data7)){
validate60.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate60.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.state !== undefined){
let data8 = data.state;
const _errs19 = errors;
if(typeof data8 !== "string"){
validate60.errors = [{instancePath:instancePath+"/state",schemaPath:"#/components/schemas/BriefState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data8 === "DRAFT") || (data8 === "FROZEN"))){
validate60.errors = [{instancePath:instancePath+"/state",schemaPath:"#/components/schemas/BriefState/enum",keyword:"enum",params:{allowedValues: schema133.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs19 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.supersedes_id !== undefined){
let data9 = data.supersedes_id;
const _errs22 = errors;
const _errs23 = errors;
let valid6 = false;
let passing0 = null;
const _errs24 = errors;
if(data9 !== null){
const err0 = {instancePath:instancePath+"/supersedes_id",schemaPath:"#/properties/supersedes_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs24 === errors;
if(_valid0){
valid6 = true;
passing0 = 0;
}
const _errs26 = errors;
const _errs27 = errors;
if(errors === _errs27){
if(errors === _errs27){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
const err1 = {instancePath:instancePath+"/supersedes_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data9))){
const err2 = {instancePath:instancePath+"/supersedes_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/supersedes_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs26 === errors;
if(_valid0 && valid6){
valid6 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid6 = true;
passing0 = 1;
}
}
if(!valid6){
const err4 = {instancePath:instancePath+"/supersedes_id",schemaPath:"#/properties/supersedes_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate60.errors = vErrors;
return false;
}
else {
errors = _errs23;
if(vErrors !== null){
if(_errs23){
vErrors.length = _errs23;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs22 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.updated_at !== undefined){
let data10 = data.updated_at;
const _errs29 = errors;
if(errors === _errs29){
if(errors === _errs29){
if(typeof data10 === "string"){
if(!(formats0.validate(data10))){
validate60.errors = [{instancePath:instancePath+"/updated_at",schemaPath:"#/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate60.errors = [{instancePath:instancePath+"/updated_at",schemaPath:"#/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs29 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.version !== undefined){
let data11 = data.version;
const _errs31 = errors;
if(!((typeof data11 == "number") && (!(data11 % 1) && !isNaN(data11)))){
validate60.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs31){
if(typeof data11 == "number"){
if(data11 > 2147483647 || isNaN(data11)){
validate60.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data11 < 1 || isNaN(data11)){
validate60.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data11))){
validate60.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs31 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate60.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate60.errors = vErrors;
return errors === 0;
}
validate60.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate59(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate59.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate60(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate60.errors : vErrors.concat(validate60.errors);
errors = vErrors.length;
}
validate59.errors = vErrors;
return errors === 0;
}
validate59.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response13 = validate66;
const schema135 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1briefs~1{id}/patch/responses/200/content/application~1json/schema"};
const schema136 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"bindings":{"items":{"$ref":"#/components/schemas/BriefBindingV1"},"maxItems":64,"minItems":1,"type":"array"},"content":{"$ref":"#/components/schemas/BriefContentV1"},"created_at":{"format":"date-time","type":"string"},"frozen_at":{"format":"date-time","type":["string","null"]},"id":{"$ref":"#/components/schemas/Id"},"project_id":{"$ref":"#/components/schemas/Id"},"revision":{"$ref":"#/components/schemas/Revision"},"state":{"$ref":"#/components/schemas/BriefState"},"supersedes_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"updated_at":{"format":"date-time","type":"string"},"version":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"}},"required":["id","project_id","version","revision","state","content","bindings","created_at","updated_at"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};

function validate67(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate67.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate67.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate67.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate67.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.version === undefined) && (missing1 = "version"))) || ((data1.revision === undefined) && (missing1 = "revision"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.content === undefined) && (missing1 = "content"))) || ((data1.bindings === undefined) && (missing1 = "bindings"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))){
validate67.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema136.properties.resource.properties, key1))){
validate67.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.bindings !== undefined){
let data2 = data1.bindings;
const _errs7 = errors;
if(errors === _errs7){
if(Array.isArray(data2)){
if(data2.length > 64){
validate67.errors = [{instancePath:instancePath+"/resource/bindings",schemaPath:"#/properties/resource/properties/bindings/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data2.length < 1){
validate67.errors = [{instancePath:instancePath+"/resource/bindings",schemaPath:"#/properties/resource/properties/bindings/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid2 = true;
const len0 = data2.length;
for(let i0=0; i0<len0; i0++){
const _errs9 = errors;
if(!(validate61(data2[i0], {instancePath:instancePath+"/resource/bindings/" + i0,parentData:data2,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate61.errors : vErrors.concat(validate61.errors);
errors = vErrors.length;
}
var valid2 = _errs9 === errors;
if(!valid2){
break;
}
}
}
}
}
else {
validate67.errors = [{instancePath:instancePath+"/resource/bindings",schemaPath:"#/properties/resource/properties/bindings/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.content !== undefined){
const _errs10 = errors;
if(!(validate63(data1.content, {instancePath:instancePath+"/resource/content",parentData:data1,parentDataProperty:"content",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate63.errors : vErrors.concat(validate63.errors);
errors = vErrors.length;
}
var valid1 = _errs10 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_at !== undefined){
let data5 = data1.created_at;
const _errs11 = errors;
if(errors === _errs11){
if(errors === _errs11){
if(typeof data5 === "string"){
if(!(formats0.validate(data5))){
validate67.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate67.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs11 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.frozen_at !== undefined){
let data6 = data1.frozen_at;
const _errs13 = errors;
if((typeof data6 !== "string") && (data6 !== null)){
validate67.errors = [{instancePath:instancePath+"/resource/frozen_at",schemaPath:"#/properties/resource/properties/frozen_at/type",keyword:"type",params:{type: schema136.properties.resource.properties.frozen_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs13){
if(errors === _errs13){
if(typeof data6 === "string"){
if(!(formats0.validate(data6))){
validate67.errors = [{instancePath:instancePath+"/resource/frozen_at",schemaPath:"#/properties/resource/properties/frozen_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs13 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data7 = data1.id;
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate67.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate67.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate67.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs15 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.project_id !== undefined){
let data8 = data1.project_id;
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate67.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate67.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate67.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs18 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revision !== undefined){
let data9 = data1.revision;
const _errs21 = errors;
const _errs22 = errors;
if(errors === _errs22){
if(typeof data9 === "string"){
if(func2(data9) > 19){
validate67.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data9) < 1){
validate67.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data9)){
validate67.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate67.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs21 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.state !== undefined){
let data10 = data1.state;
const _errs24 = errors;
if(typeof data10 !== "string"){
validate67.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/BriefState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data10 === "DRAFT") || (data10 === "FROZEN"))){
validate67.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/BriefState/enum",keyword:"enum",params:{allowedValues: schema133.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs24 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.supersedes_id !== undefined){
let data11 = data1.supersedes_id;
const _errs27 = errors;
const _errs28 = errors;
let valid7 = false;
let passing0 = null;
const _errs29 = errors;
if(data11 !== null){
const err0 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/properties/resource/properties/supersedes_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs29 === errors;
if(_valid0){
valid7 = true;
passing0 = 0;
}
const _errs31 = errors;
const _errs32 = errors;
if(errors === _errs32){
if(errors === _errs32){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err1 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err2 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs31 === errors;
if(_valid0 && valid7){
valid7 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid7 = true;
passing0 = 1;
}
}
if(!valid7){
const err4 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/properties/resource/properties/supersedes_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate67.errors = vErrors;
return false;
}
else {
errors = _errs28;
if(vErrors !== null){
if(_errs28){
vErrors.length = _errs28;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs27 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.updated_at !== undefined){
let data12 = data1.updated_at;
const _errs34 = errors;
if(errors === _errs34){
if(errors === _errs34){
if(typeof data12 === "string"){
if(!(formats0.validate(data12))){
validate67.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate67.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs34 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.version !== undefined){
let data13 = data1.version;
const _errs36 = errors;
if(!((typeof data13 == "number") && (!(data13 % 1) && !isNaN(data13)))){
validate67.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs36){
if(typeof data13 == "number"){
if(data13 > 2147483647 || isNaN(data13)){
validate67.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data13 < 1 || isNaN(data13)){
validate67.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data13))){
validate67.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid1 = _errs36 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate67.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data14 = data.schema_version;
const _errs38 = errors;
const _errs39 = errors;
if(!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))){
validate67.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data14 === 1)){
validate67.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs39){
if(typeof data14 == "number"){
if(data14 > 1 || isNaN(data14)){
validate67.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data14 < 1 || isNaN(data14)){
validate67.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs38 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate67.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate67.errors = vErrors;
return errors === 0;
}
validate67.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate66(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate66.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate67(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate67.errors : vErrors.concat(validate67.errors);
errors = vErrors.length;
}
validate66.errors = vErrors;
return errors === 0;
}
validate66.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response14 = validate71;
const schema143 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1evaluation-policies/get/responses/200/content/application~1json/schema"};
const schema144 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"created_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"maximum_missing_fraction":{"allOf":[{"description":"Plain decimal exactly representable by NUMERIC(38,18).","maxLength":64,"minLength":1,"pattern":"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])","type":"string"},{"pattern":"^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])","type":"string"}]},"maximum_sealed_uses_per_lineage":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"},"metric_requirements":{"items":{"$ref":"#/components/schemas/MetricRequirementV1"},"maxItems":64,"minItems":1,"type":"array"},"minimum_observations":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"},"project_id":{"$ref":"#/components/schemas/Id"},"question":{"maxLength":8000,"minLength":1,"type":"string"},"require_real_data":{"type":"boolean"},"required_capabilities":{"items":{"maxLength":120,"minLength":1,"type":"string"},"maxItems":64,"minItems":0,"type":"array","uniqueItems":true},"selection_rule":{"$ref":"#/components/schemas/SelectionRuleV1"},"split_policy":{"$ref":"#/components/schemas/SplitPolicyV1"},"validity_seconds":{"$ref":"#/components/schemas/DbCounter"},"version":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"}},"required":["id","project_id","version","created_at","question","selection_rule","split_policy","metric_requirements","minimum_observations","maximum_missing_fraction","require_real_data","required_capabilities","maximum_sealed_uses_per_lineage","validity_seconds"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};
const pattern75 = new RegExp("^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])", "u");
const schema146 = {"additionalProperties":false,"properties":{"comparator":{"$ref":"#/components/schemas/Comparator"},"method_allowlist":{"items":{"maxLength":120,"minLength":1,"type":"string"},"maxItems":64,"minItems":1,"type":"array","uniqueItems":true},"metric_code":{"maxLength":120,"minLength":1,"type":"string"},"minimum_observations":{"$ref":"#/components/schemas/DbCounter"},"required":{"type":"boolean"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"scope":{"maxLength":120,"minLength":1,"type":"string"},"threshold_high":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/DecimalValue"}]},"threshold_low":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/DecimalValue"}]}},"required":["schema_version","metric_code","scope","comparator","required","minimum_observations","method_allowlist"],"type":"object"};
const schema147 = {"enum":["GT","GE","LT","LE","BETWEEN"],"type":"string"};

function validate73(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate73.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.metric_code === undefined) && (missing0 = "metric_code"))) || ((data.scope === undefined) && (missing0 = "scope"))) || ((data.comparator === undefined) && (missing0 = "comparator"))) || ((data.required === undefined) && (missing0 = "required"))) || ((data.minimum_observations === undefined) && (missing0 = "minimum_observations"))) || ((data.method_allowlist === undefined) && (missing0 = "method_allowlist"))){
validate73.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema146.properties, key0))){
validate73.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.comparator !== undefined){
let data0 = data.comparator;
const _errs2 = errors;
if(typeof data0 !== "string"){
validate73.errors = [{instancePath:instancePath+"/comparator",schemaPath:"#/components/schemas/Comparator/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(((((data0 === "GT") || (data0 === "GE")) || (data0 === "LT")) || (data0 === "LE")) || (data0 === "BETWEEN"))){
validate73.errors = [{instancePath:instancePath+"/comparator",schemaPath:"#/components/schemas/Comparator/enum",keyword:"enum",params:{allowedValues: schema147.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.method_allowlist !== undefined){
let data1 = data.method_allowlist;
const _errs5 = errors;
if(errors === _errs5){
if(Array.isArray(data1)){
if(data1.length > 64){
validate73.errors = [{instancePath:instancePath+"/method_allowlist",schemaPath:"#/properties/method_allowlist/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data1.length < 1){
validate73.errors = [{instancePath:instancePath+"/method_allowlist",schemaPath:"#/properties/method_allowlist/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid2 = true;
const len0 = data1.length;
for(let i0=0; i0<len0; i0++){
let data2 = data1[i0];
const _errs7 = errors;
if(errors === _errs7){
if(typeof data2 === "string"){
if(func2(data2) > 120){
validate73.errors = [{instancePath:instancePath+"/method_allowlist/" + i0,schemaPath:"#/properties/method_allowlist/items/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data2) < 1){
validate73.errors = [{instancePath:instancePath+"/method_allowlist/" + i0,schemaPath:"#/properties/method_allowlist/items/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate73.errors = [{instancePath:instancePath+"/method_allowlist/" + i0,schemaPath:"#/properties/method_allowlist/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs7 === errors;
if(!valid2){
break;
}
}
if(valid2){
let i1 = data1.length;
let j0;
if(i1 > 1){
const indices0 = {};
for(;i1--;){
let item0 = data1[i1];
if(typeof item0 !== "string"){
continue;
}
if(typeof indices0[item0] == "number"){
j0 = indices0[item0];
validate73.errors = [{instancePath:instancePath+"/method_allowlist",schemaPath:"#/properties/method_allowlist/uniqueItems",keyword:"uniqueItems",params:{i: i1, j: j0},message:"must NOT have duplicate items (items ## "+j0+" and "+i1+" are identical)"}];
return false;
break;
}
indices0[item0] = i1;
}
}
}
}
}
}
else {
validate73.errors = [{instancePath:instancePath+"/method_allowlist",schemaPath:"#/properties/method_allowlist/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.metric_code !== undefined){
let data3 = data.metric_code;
const _errs9 = errors;
if(errors === _errs9){
if(typeof data3 === "string"){
if(func2(data3) > 120){
validate73.errors = [{instancePath:instancePath+"/metric_code",schemaPath:"#/properties/metric_code/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data3) < 1){
validate73.errors = [{instancePath:instancePath+"/metric_code",schemaPath:"#/properties/metric_code/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate73.errors = [{instancePath:instancePath+"/metric_code",schemaPath:"#/properties/metric_code/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.minimum_observations !== undefined){
let data4 = data.minimum_observations;
const _errs11 = errors;
const _errs12 = errors;
if(errors === _errs12){
if(typeof data4 === "string"){
if(func2(data4) > 19){
validate73.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data4) < 1){
validate73.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data4)){
validate73.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate73.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.required !== undefined){
const _errs14 = errors;
if(typeof data.required !== "boolean"){
validate73.errors = [{instancePath:instancePath+"/required",schemaPath:"#/properties/required/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs14 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data6 = data.schema_version;
const _errs16 = errors;
const _errs17 = errors;
if(!((typeof data6 == "number") && (!(data6 % 1) && !isNaN(data6)))){
validate73.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data6 === 1)){
validate73.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs17){
if(typeof data6 == "number"){
if(data6 > 1 || isNaN(data6)){
validate73.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data6 < 1 || isNaN(data6)){
validate73.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.scope !== undefined){
let data7 = data.scope;
const _errs19 = errors;
if(errors === _errs19){
if(typeof data7 === "string"){
if(func2(data7) > 120){
validate73.errors = [{instancePath:instancePath+"/scope",schemaPath:"#/properties/scope/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data7) < 1){
validate73.errors = [{instancePath:instancePath+"/scope",schemaPath:"#/properties/scope/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate73.errors = [{instancePath:instancePath+"/scope",schemaPath:"#/properties/scope/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs19 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.threshold_high !== undefined){
let data8 = data.threshold_high;
const _errs21 = errors;
const _errs22 = errors;
let valid6 = false;
let passing0 = null;
const _errs23 = errors;
if(data8 !== null){
const err0 = {instancePath:instancePath+"/threshold_high",schemaPath:"#/properties/threshold_high/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs23 === errors;
if(_valid0){
valid6 = true;
passing0 = 0;
}
const _errs25 = errors;
const _errs26 = errors;
if(errors === _errs26){
if(typeof data8 === "string"){
if(func2(data8) > 64){
const err1 = {instancePath:instancePath+"/threshold_high",schemaPath:"#/components/schemas/DecimalValue/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(func2(data8) < 1){
const err2 = {instancePath:instancePath+"/threshold_high",schemaPath:"#/components/schemas/DecimalValue/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
else {
if(!pattern38.test(data8)){
const err3 = {instancePath:instancePath+"/threshold_high",schemaPath:"#/components/schemas/DecimalValue/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
}
else {
const err4 = {instancePath:instancePath+"/threshold_high",schemaPath:"#/components/schemas/DecimalValue/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
}
var _valid0 = _errs25 === errors;
if(_valid0 && valid6){
valid6 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid6 = true;
passing0 = 1;
}
}
if(!valid6){
const err5 = {instancePath:instancePath+"/threshold_high",schemaPath:"#/properties/threshold_high/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
validate73.errors = vErrors;
return false;
}
else {
errors = _errs22;
if(vErrors !== null){
if(_errs22){
vErrors.length = _errs22;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs21 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.threshold_low !== undefined){
let data9 = data.threshold_low;
const _errs28 = errors;
const _errs29 = errors;
let valid8 = false;
let passing1 = null;
const _errs30 = errors;
if(data9 !== null){
const err6 = {instancePath:instancePath+"/threshold_low",schemaPath:"#/properties/threshold_low/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
var _valid1 = _errs30 === errors;
if(_valid1){
valid8 = true;
passing1 = 0;
}
const _errs32 = errors;
const _errs33 = errors;
if(errors === _errs33){
if(typeof data9 === "string"){
if(func2(data9) > 64){
const err7 = {instancePath:instancePath+"/threshold_low",schemaPath:"#/components/schemas/DecimalValue/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
else {
if(func2(data9) < 1){
const err8 = {instancePath:instancePath+"/threshold_low",schemaPath:"#/components/schemas/DecimalValue/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
else {
if(!pattern38.test(data9)){
const err9 = {instancePath:instancePath+"/threshold_low",schemaPath:"#/components/schemas/DecimalValue/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
}
}
}
}
else {
const err10 = {instancePath:instancePath+"/threshold_low",schemaPath:"#/components/schemas/DecimalValue/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
}
var _valid1 = _errs32 === errors;
if(_valid1 && valid8){
valid8 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid8 = true;
passing1 = 1;
}
}
if(!valid8){
const err11 = {instancePath:instancePath+"/threshold_low",schemaPath:"#/properties/threshold_low/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
validate73.errors = vErrors;
return false;
}
else {
errors = _errs29;
if(vErrors !== null){
if(_errs29){
vErrors.length = _errs29;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs28 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate73.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate73.errors = vErrors;
return errors === 0;
}
validate73.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

const schema153 = {"additionalProperties":false,"properties":{"candidate_count":{"format":"int32","maximum":65535,"minimum":1,"type":"integer"},"comparable_scope":{"$ref":"#/components/schemas/ComparableScope"},"comparison_input_set_id":{"$ref":"#/components/schemas/Id"},"direction":{"$ref":"#/components/schemas/SelectionDirection"},"evaluation_kind":{"$ref":"#/components/schemas/SelectionEvaluationKind"},"execution_assumptions_id":{"$ref":"#/components/schemas/Id"},"family_id":{"$ref":"#/components/schemas/Id"},"frequency":{"maxLength":120,"minLength":1,"type":"string"},"method_id":{"maxLength":120,"minLength":1,"type":"string"},"method_version":{"maxLength":120,"minLength":1,"type":"string"},"metric_code":{"maxLength":120,"minLength":1,"type":"string"},"metric_scope":{"maxLength":120,"minLength":1,"type":"string"},"missing_required_metric":{"$ref":"#/components/schemas/MissingSelectionMetric"},"root_lineage_id":{"$ref":"#/components/schemas/Id"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"tie_break":{"$ref":"#/components/schemas/SelectionTieBreak"},"unit":{"maxLength":120,"minLength":1,"type":"string"}},"required":["schema_version","comparable_scope","root_lineage_id","family_id","comparison_input_set_id","execution_assumptions_id","evaluation_kind","metric_code","metric_scope","method_id","method_version","unit","frequency","direction","candidate_count","tie_break","missing_required_metric"],"type":"object"};
const schema154 = {"enum":["FAMILY_LINEAGE"],"type":"string"};
const schema156 = {"enum":["MAXIMIZE","MINIMIZE"],"type":"string"};
const schema157 = {"enum":["WALK_FORWARD","SEALED"],"type":"string"};
const schema160 = {"enum":["INCONCLUSIVE"],"type":"string"};
const schema163 = {"enum":["EXPERIMENT_ID_ASC"],"type":"string"};

function validate75(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate75.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((((((((((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.comparable_scope === undefined) && (missing0 = "comparable_scope"))) || ((data.root_lineage_id === undefined) && (missing0 = "root_lineage_id"))) || ((data.family_id === undefined) && (missing0 = "family_id"))) || ((data.comparison_input_set_id === undefined) && (missing0 = "comparison_input_set_id"))) || ((data.execution_assumptions_id === undefined) && (missing0 = "execution_assumptions_id"))) || ((data.evaluation_kind === undefined) && (missing0 = "evaluation_kind"))) || ((data.metric_code === undefined) && (missing0 = "metric_code"))) || ((data.metric_scope === undefined) && (missing0 = "metric_scope"))) || ((data.method_id === undefined) && (missing0 = "method_id"))) || ((data.method_version === undefined) && (missing0 = "method_version"))) || ((data.unit === undefined) && (missing0 = "unit"))) || ((data.frequency === undefined) && (missing0 = "frequency"))) || ((data.direction === undefined) && (missing0 = "direction"))) || ((data.candidate_count === undefined) && (missing0 = "candidate_count"))) || ((data.tie_break === undefined) && (missing0 = "tie_break"))) || ((data.missing_required_metric === undefined) && (missing0 = "missing_required_metric"))){
validate75.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema153.properties, key0))){
validate75.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.candidate_count !== undefined){
let data0 = data.candidate_count;
const _errs2 = errors;
if(!((typeof data0 == "number") && (!(data0 % 1) && !isNaN(data0)))){
validate75.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs2){
if(typeof data0 == "number"){
if(data0 > 65535 || isNaN(data0)){
validate75.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data0 < 1 || isNaN(data0)){
validate75.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data0))){
validate75.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.comparable_scope !== undefined){
let data1 = data.comparable_scope;
const _errs4 = errors;
if(typeof data1 !== "string"){
validate75.errors = [{instancePath:instancePath+"/comparable_scope",schemaPath:"#/components/schemas/ComparableScope/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(data1 === "FAMILY_LINEAGE")){
validate75.errors = [{instancePath:instancePath+"/comparable_scope",schemaPath:"#/components/schemas/ComparableScope/enum",keyword:"enum",params:{allowedValues: schema154.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.comparison_input_set_id !== undefined){
let data2 = data.comparison_input_set_id;
const _errs7 = errors;
const _errs8 = errors;
if(errors === _errs8){
if(errors === _errs8){
if(typeof data2 === "string"){
if(!pattern5.test(data2)){
validate75.errors = [{instancePath:instancePath+"/comparison_input_set_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data2))){
validate75.errors = [{instancePath:instancePath+"/comparison_input_set_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/comparison_input_set_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.direction !== undefined){
let data3 = data.direction;
const _errs10 = errors;
if(typeof data3 !== "string"){
validate75.errors = [{instancePath:instancePath+"/direction",schemaPath:"#/components/schemas/SelectionDirection/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data3 === "MAXIMIZE") || (data3 === "MINIMIZE"))){
validate75.errors = [{instancePath:instancePath+"/direction",schemaPath:"#/components/schemas/SelectionDirection/enum",keyword:"enum",params:{allowedValues: schema156.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs10 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.evaluation_kind !== undefined){
let data4 = data.evaluation_kind;
const _errs13 = errors;
if(typeof data4 !== "string"){
validate75.errors = [{instancePath:instancePath+"/evaluation_kind",schemaPath:"#/components/schemas/SelectionEvaluationKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data4 === "WALK_FORWARD") || (data4 === "SEALED"))){
validate75.errors = [{instancePath:instancePath+"/evaluation_kind",schemaPath:"#/components/schemas/SelectionEvaluationKind/enum",keyword:"enum",params:{allowedValues: schema157.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.execution_assumptions_id !== undefined){
let data5 = data.execution_assumptions_id;
const _errs16 = errors;
const _errs17 = errors;
if(errors === _errs17){
if(errors === _errs17){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
validate75.errors = [{instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data5))){
validate75.errors = [{instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/execution_assumptions_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.family_id !== undefined){
let data6 = data.family_id;
const _errs19 = errors;
const _errs20 = errors;
if(errors === _errs20){
if(errors === _errs20){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate75.errors = [{instancePath:instancePath+"/family_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate75.errors = [{instancePath:instancePath+"/family_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/family_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs19 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.frequency !== undefined){
let data7 = data.frequency;
const _errs22 = errors;
if(errors === _errs22){
if(typeof data7 === "string"){
if(func2(data7) > 120){
validate75.errors = [{instancePath:instancePath+"/frequency",schemaPath:"#/properties/frequency/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data7) < 1){
validate75.errors = [{instancePath:instancePath+"/frequency",schemaPath:"#/properties/frequency/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/frequency",schemaPath:"#/properties/frequency/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs22 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.method_id !== undefined){
let data8 = data.method_id;
const _errs24 = errors;
if(errors === _errs24){
if(typeof data8 === "string"){
if(func2(data8) > 120){
validate75.errors = [{instancePath:instancePath+"/method_id",schemaPath:"#/properties/method_id/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data8) < 1){
validate75.errors = [{instancePath:instancePath+"/method_id",schemaPath:"#/properties/method_id/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/method_id",schemaPath:"#/properties/method_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs24 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.method_version !== undefined){
let data9 = data.method_version;
const _errs26 = errors;
if(errors === _errs26){
if(typeof data9 === "string"){
if(func2(data9) > 120){
validate75.errors = [{instancePath:instancePath+"/method_version",schemaPath:"#/properties/method_version/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data9) < 1){
validate75.errors = [{instancePath:instancePath+"/method_version",schemaPath:"#/properties/method_version/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/method_version",schemaPath:"#/properties/method_version/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs26 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.metric_code !== undefined){
let data10 = data.metric_code;
const _errs28 = errors;
if(errors === _errs28){
if(typeof data10 === "string"){
if(func2(data10) > 120){
validate75.errors = [{instancePath:instancePath+"/metric_code",schemaPath:"#/properties/metric_code/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate75.errors = [{instancePath:instancePath+"/metric_code",schemaPath:"#/properties/metric_code/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/metric_code",schemaPath:"#/properties/metric_code/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs28 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.metric_scope !== undefined){
let data11 = data.metric_scope;
const _errs30 = errors;
if(errors === _errs30){
if(typeof data11 === "string"){
if(func2(data11) > 120){
validate75.errors = [{instancePath:instancePath+"/metric_scope",schemaPath:"#/properties/metric_scope/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data11) < 1){
validate75.errors = [{instancePath:instancePath+"/metric_scope",schemaPath:"#/properties/metric_scope/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/metric_scope",schemaPath:"#/properties/metric_scope/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs30 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.missing_required_metric !== undefined){
let data12 = data.missing_required_metric;
const _errs32 = errors;
if(typeof data12 !== "string"){
validate75.errors = [{instancePath:instancePath+"/missing_required_metric",schemaPath:"#/components/schemas/MissingSelectionMetric/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(data12 === "INCONCLUSIVE")){
validate75.errors = [{instancePath:instancePath+"/missing_required_metric",schemaPath:"#/components/schemas/MissingSelectionMetric/enum",keyword:"enum",params:{allowedValues: schema160.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs32 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.root_lineage_id !== undefined){
let data13 = data.root_lineage_id;
const _errs35 = errors;
const _errs36 = errors;
if(errors === _errs36){
if(errors === _errs36){
if(typeof data13 === "string"){
if(!pattern5.test(data13)){
validate75.errors = [{instancePath:instancePath+"/root_lineage_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data13))){
validate75.errors = [{instancePath:instancePath+"/root_lineage_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/root_lineage_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs35 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data14 = data.schema_version;
const _errs38 = errors;
const _errs39 = errors;
if(!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))){
validate75.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data14 === 1)){
validate75.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs39){
if(typeof data14 == "number"){
if(data14 > 1 || isNaN(data14)){
validate75.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data14 < 1 || isNaN(data14)){
validate75.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs38 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.tie_break !== undefined){
let data15 = data.tie_break;
const _errs41 = errors;
if(typeof data15 !== "string"){
validate75.errors = [{instancePath:instancePath+"/tie_break",schemaPath:"#/components/schemas/SelectionTieBreak/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(data15 === "EXPERIMENT_ID_ASC")){
validate75.errors = [{instancePath:instancePath+"/tie_break",schemaPath:"#/components/schemas/SelectionTieBreak/enum",keyword:"enum",params:{allowedValues: schema163.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs41 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.unit !== undefined){
let data16 = data.unit;
const _errs44 = errors;
if(errors === _errs44){
if(typeof data16 === "string"){
if(func2(data16) > 120){
validate75.errors = [{instancePath:instancePath+"/unit",schemaPath:"#/properties/unit/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data16) < 1){
validate75.errors = [{instancePath:instancePath+"/unit",schemaPath:"#/properties/unit/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate75.errors = [{instancePath:instancePath+"/unit",schemaPath:"#/properties/unit/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs44 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate75.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate75.errors = vErrors;
return errors === 0;
}
validate75.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

const schema164 = {"additionalProperties":false,"properties":{"embargo_observations":{"$ref":"#/components/schemas/DbCounter"},"group_count":{"format":"int32","maximum":65535,"minimum":2,"type":["integer","null"]},"interval_validation_required":{"enum":[true],"type":"boolean"},"kind":{"$ref":"#/components/schemas/SplitKind"},"label_horizon_observations":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/DbCounter"}]},"purge_observations":{"$ref":"#/components/schemas/DbCounter"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"sealed_revision_id":{"$ref":"#/components/schemas/Id"},"step_size":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/DbCounter"}]},"test_group_count":{"format":"int32","maximum":65534,"minimum":1,"type":["integer","null"]},"test_size":{"$ref":"#/components/schemas/DbCounter"},"train_size":{"$ref":"#/components/schemas/DbCounter"}},"required":["schema_version","kind","train_size","test_size","purge_observations","embargo_observations","interval_validation_required","sealed_revision_id"],"type":"object"};
const schema166 = {"enum":["WALK_FORWARD","CPCV_FIXED_HORIZON"],"type":"string"};

function validate77(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate77.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.kind === undefined) && (missing0 = "kind"))) || ((data.train_size === undefined) && (missing0 = "train_size"))) || ((data.test_size === undefined) && (missing0 = "test_size"))) || ((data.purge_observations === undefined) && (missing0 = "purge_observations"))) || ((data.embargo_observations === undefined) && (missing0 = "embargo_observations"))) || ((data.interval_validation_required === undefined) && (missing0 = "interval_validation_required"))) || ((data.sealed_revision_id === undefined) && (missing0 = "sealed_revision_id"))){
validate77.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema164.properties, key0))){
validate77.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.embargo_observations !== undefined){
let data0 = data.embargo_observations;
const _errs2 = errors;
const _errs3 = errors;
if(errors === _errs3){
if(typeof data0 === "string"){
if(func2(data0) > 19){
validate77.errors = [{instancePath:instancePath+"/embargo_observations",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data0) < 1){
validate77.errors = [{instancePath:instancePath+"/embargo_observations",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data0)){
validate77.errors = [{instancePath:instancePath+"/embargo_observations",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate77.errors = [{instancePath:instancePath+"/embargo_observations",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.group_count !== undefined){
let data1 = data.group_count;
const _errs5 = errors;
if((!((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1)))) && (data1 !== null)){
validate77.errors = [{instancePath:instancePath+"/group_count",schemaPath:"#/properties/group_count/type",keyword:"type",params:{type: schema164.properties.group_count.type},message:"must be integer,null"}];
return false;
}
if(errors === _errs5){
if(typeof data1 == "number"){
if(data1 > 65535 || isNaN(data1)){
validate77.errors = [{instancePath:instancePath+"/group_count",schemaPath:"#/properties/group_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data1 < 2 || isNaN(data1)){
validate77.errors = [{instancePath:instancePath+"/group_count",schemaPath:"#/properties/group_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 2},message:"must be >= 2"}];
return false;
}
else {
if(!(formats94.validate(data1))){
validate77.errors = [{instancePath:instancePath+"/group_count",schemaPath:"#/properties/group_count/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.interval_validation_required !== undefined){
let data2 = data.interval_validation_required;
const _errs7 = errors;
if(typeof data2 !== "boolean"){
validate77.errors = [{instancePath:instancePath+"/interval_validation_required",schemaPath:"#/properties/interval_validation_required/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
if(!(data2 === true)){
validate77.errors = [{instancePath:instancePath+"/interval_validation_required",schemaPath:"#/properties/interval_validation_required/enum",keyword:"enum",params:{allowedValues: schema164.properties.interval_validation_required.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.kind !== undefined){
let data3 = data.kind;
const _errs9 = errors;
if(typeof data3 !== "string"){
validate77.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/components/schemas/SplitKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data3 === "WALK_FORWARD") || (data3 === "CPCV_FIXED_HORIZON"))){
validate77.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/components/schemas/SplitKind/enum",keyword:"enum",params:{allowedValues: schema166.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.label_horizon_observations !== undefined){
let data4 = data.label_horizon_observations;
const _errs12 = errors;
const _errs13 = errors;
let valid3 = false;
let passing0 = null;
const _errs14 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/label_horizon_observations",schemaPath:"#/properties/label_horizon_observations/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs14 === errors;
if(_valid0){
valid3 = true;
passing0 = 0;
}
const _errs16 = errors;
const _errs17 = errors;
if(errors === _errs17){
if(typeof data4 === "string"){
if(func2(data4) > 19){
const err1 = {instancePath:instancePath+"/label_horizon_observations",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(func2(data4) < 1){
const err2 = {instancePath:instancePath+"/label_horizon_observations",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
else {
if(!pattern4.test(data4)){
const err3 = {instancePath:instancePath+"/label_horizon_observations",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
}
else {
const err4 = {instancePath:instancePath+"/label_horizon_observations",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
}
var _valid0 = _errs16 === errors;
if(_valid0 && valid3){
valid3 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid3 = true;
passing0 = 1;
}
}
if(!valid3){
const err5 = {instancePath:instancePath+"/label_horizon_observations",schemaPath:"#/properties/label_horizon_observations/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
validate77.errors = vErrors;
return false;
}
else {
errors = _errs13;
if(vErrors !== null){
if(_errs13){
vErrors.length = _errs13;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs12 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.purge_observations !== undefined){
let data5 = data.purge_observations;
const _errs19 = errors;
const _errs20 = errors;
if(errors === _errs20){
if(typeof data5 === "string"){
if(func2(data5) > 19){
validate77.errors = [{instancePath:instancePath+"/purge_observations",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data5) < 1){
validate77.errors = [{instancePath:instancePath+"/purge_observations",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data5)){
validate77.errors = [{instancePath:instancePath+"/purge_observations",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate77.errors = [{instancePath:instancePath+"/purge_observations",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs19 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data6 = data.schema_version;
const _errs22 = errors;
const _errs23 = errors;
if(!((typeof data6 == "number") && (!(data6 % 1) && !isNaN(data6)))){
validate77.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data6 === 1)){
validate77.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs23){
if(typeof data6 == "number"){
if(data6 > 1 || isNaN(data6)){
validate77.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data6 < 1 || isNaN(data6)){
validate77.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs22 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.sealed_revision_id !== undefined){
let data7 = data.sealed_revision_id;
const _errs25 = errors;
const _errs26 = errors;
if(errors === _errs26){
if(errors === _errs26){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate77.errors = [{instancePath:instancePath+"/sealed_revision_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate77.errors = [{instancePath:instancePath+"/sealed_revision_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate77.errors = [{instancePath:instancePath+"/sealed_revision_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs25 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.step_size !== undefined){
let data8 = data.step_size;
const _errs28 = errors;
const _errs29 = errors;
let valid8 = false;
let passing1 = null;
const _errs30 = errors;
if(data8 !== null){
const err6 = {instancePath:instancePath+"/step_size",schemaPath:"#/properties/step_size/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
var _valid1 = _errs30 === errors;
if(_valid1){
valid8 = true;
passing1 = 0;
}
const _errs32 = errors;
const _errs33 = errors;
if(errors === _errs33){
if(typeof data8 === "string"){
if(func2(data8) > 19){
const err7 = {instancePath:instancePath+"/step_size",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
else {
if(func2(data8) < 1){
const err8 = {instancePath:instancePath+"/step_size",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
else {
if(!pattern4.test(data8)){
const err9 = {instancePath:instancePath+"/step_size",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
}
}
}
}
else {
const err10 = {instancePath:instancePath+"/step_size",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
}
var _valid1 = _errs32 === errors;
if(_valid1 && valid8){
valid8 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid8 = true;
passing1 = 1;
}
}
if(!valid8){
const err11 = {instancePath:instancePath+"/step_size",schemaPath:"#/properties/step_size/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
validate77.errors = vErrors;
return false;
}
else {
errors = _errs29;
if(vErrors !== null){
if(_errs29){
vErrors.length = _errs29;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs28 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.test_group_count !== undefined){
let data9 = data.test_group_count;
const _errs35 = errors;
if((!((typeof data9 == "number") && (!(data9 % 1) && !isNaN(data9)))) && (data9 !== null)){
validate77.errors = [{instancePath:instancePath+"/test_group_count",schemaPath:"#/properties/test_group_count/type",keyword:"type",params:{type: schema164.properties.test_group_count.type},message:"must be integer,null"}];
return false;
}
if(errors === _errs35){
if(typeof data9 == "number"){
if(data9 > 65534 || isNaN(data9)){
validate77.errors = [{instancePath:instancePath+"/test_group_count",schemaPath:"#/properties/test_group_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65534},message:"must be <= 65534"}];
return false;
}
else {
if(data9 < 1 || isNaN(data9)){
validate77.errors = [{instancePath:instancePath+"/test_group_count",schemaPath:"#/properties/test_group_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data9))){
validate77.errors = [{instancePath:instancePath+"/test_group_count",schemaPath:"#/properties/test_group_count/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs35 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.test_size !== undefined){
let data10 = data.test_size;
const _errs37 = errors;
const _errs38 = errors;
if(errors === _errs38){
if(typeof data10 === "string"){
if(func2(data10) > 19){
validate77.errors = [{instancePath:instancePath+"/test_size",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate77.errors = [{instancePath:instancePath+"/test_size",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data10)){
validate77.errors = [{instancePath:instancePath+"/test_size",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate77.errors = [{instancePath:instancePath+"/test_size",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs37 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.train_size !== undefined){
let data11 = data.train_size;
const _errs40 = errors;
const _errs41 = errors;
if(errors === _errs41){
if(typeof data11 === "string"){
if(func2(data11) > 19){
validate77.errors = [{instancePath:instancePath+"/train_size",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data11) < 1){
validate77.errors = [{instancePath:instancePath+"/train_size",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data11)){
validate77.errors = [{instancePath:instancePath+"/train_size",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate77.errors = [{instancePath:instancePath+"/train_size",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs40 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate77.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate77.errors = vErrors;
return errors === 0;
}
validate77.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate72(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate72.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate72.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate72.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if(((((((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.version === undefined) && (missing1 = "version"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.question === undefined) && (missing1 = "question"))) || ((data1.selection_rule === undefined) && (missing1 = "selection_rule"))) || ((data1.split_policy === undefined) && (missing1 = "split_policy"))) || ((data1.metric_requirements === undefined) && (missing1 = "metric_requirements"))) || ((data1.minimum_observations === undefined) && (missing1 = "minimum_observations"))) || ((data1.maximum_missing_fraction === undefined) && (missing1 = "maximum_missing_fraction"))) || ((data1.require_real_data === undefined) && (missing1 = "require_real_data"))) || ((data1.required_capabilities === undefined) && (missing1 = "required_capabilities"))) || ((data1.maximum_sealed_uses_per_lineage === undefined) && (missing1 = "maximum_sealed_uses_per_lineage"))) || ((data1.validity_seconds === undefined) && (missing1 = "validity_seconds"))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema144.properties.items.items.properties, key1))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.created_at !== undefined){
let data2 = data1.created_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data3 = data1.id;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data3))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs9 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.maximum_missing_fraction !== undefined){
let data4 = data1.maximum_missing_fraction;
const _errs12 = errors;
const _errs13 = errors;
if(errors === _errs13){
if(typeof data4 === "string"){
if(func2(data4) > 64){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_missing_fraction",schemaPath:"#/properties/items/items/properties/maximum_missing_fraction/allOf/0/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"}];
return false;
}
else {
if(func2(data4) < 1){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_missing_fraction",schemaPath:"#/properties/items/items/properties/maximum_missing_fraction/allOf/0/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern38.test(data4)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_missing_fraction",schemaPath:"#/properties/items/items/properties/maximum_missing_fraction/allOf/0/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_missing_fraction",schemaPath:"#/properties/items/items/properties/maximum_missing_fraction/allOf/0/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid4 = _errs13 === errors;
if(valid4){
const _errs15 = errors;
if(errors === _errs15){
if(typeof data4 === "string"){
if(!pattern75.test(data4)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_missing_fraction",schemaPath:"#/properties/items/items/properties/maximum_missing_fraction/allOf/1/pattern",keyword:"pattern",params:{pattern: "^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])"},message:"must match pattern \""+"^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])"+"\""}];
return false;
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_missing_fraction",schemaPath:"#/properties/items/items/properties/maximum_missing_fraction/allOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid4 = _errs15 === errors;
}
var valid2 = _errs12 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.maximum_sealed_uses_per_lineage !== undefined){
let data5 = data1.maximum_sealed_uses_per_lineage;
const _errs17 = errors;
if(!((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5)))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/items/items/properties/maximum_sealed_uses_per_lineage/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs17){
if(typeof data5 == "number"){
if(data5 > 2147483647 || isNaN(data5)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/items/items/properties/maximum_sealed_uses_per_lineage/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data5 < 1 || isNaN(data5)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/items/items/properties/maximum_sealed_uses_per_lineage/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data5))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/items/items/properties/maximum_sealed_uses_per_lineage/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid2 = _errs17 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.metric_requirements !== undefined){
let data6 = data1.metric_requirements;
const _errs19 = errors;
if(errors === _errs19){
if(Array.isArray(data6)){
if(data6.length > 64){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/metric_requirements",schemaPath:"#/properties/items/items/properties/metric_requirements/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data6.length < 1){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/metric_requirements",schemaPath:"#/properties/items/items/properties/metric_requirements/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid5 = true;
const len1 = data6.length;
for(let i1=0; i1<len1; i1++){
const _errs21 = errors;
if(!(validate73(data6[i1], {instancePath:instancePath+"/items/" + i0+"/metric_requirements/" + i1,parentData:data6,parentDataProperty:i1,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate73.errors : vErrors.concat(validate73.errors);
errors = vErrors.length;
}
var valid5 = _errs21 === errors;
if(!valid5){
break;
}
}
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/metric_requirements",schemaPath:"#/properties/items/items/properties/metric_requirements/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid2 = _errs19 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.minimum_observations !== undefined){
let data8 = data1.minimum_observations;
const _errs22 = errors;
if(!((typeof data8 == "number") && (!(data8 % 1) && !isNaN(data8)))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/minimum_observations",schemaPath:"#/properties/items/items/properties/minimum_observations/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs22){
if(typeof data8 == "number"){
if(data8 > 2147483647 || isNaN(data8)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/minimum_observations",schemaPath:"#/properties/items/items/properties/minimum_observations/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data8 < 1 || isNaN(data8)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/minimum_observations",schemaPath:"#/properties/items/items/properties/minimum_observations/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data8))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/minimum_observations",schemaPath:"#/properties/items/items/properties/minimum_observations/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid2 = _errs22 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.project_id !== undefined){
let data9 = data1.project_id;
const _errs24 = errors;
const _errs25 = errors;
if(errors === _errs25){
if(errors === _errs25){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data9))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs24 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.question !== undefined){
let data10 = data1.question;
const _errs27 = errors;
if(errors === _errs27){
if(typeof data10 === "string"){
if(func2(data10) > 8000){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/question",schemaPath:"#/properties/items/items/properties/question/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/question",schemaPath:"#/properties/items/items/properties/question/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/question",schemaPath:"#/properties/items/items/properties/question/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs27 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.require_real_data !== undefined){
const _errs29 = errors;
if(typeof data1.require_real_data !== "boolean"){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/require_real_data",schemaPath:"#/properties/items/items/properties/require_real_data/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid2 = _errs29 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.required_capabilities !== undefined){
let data12 = data1.required_capabilities;
const _errs31 = errors;
if(errors === _errs31){
if(Array.isArray(data12)){
if(data12.length > 64){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/required_capabilities",schemaPath:"#/properties/items/items/properties/required_capabilities/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data12.length < 0){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/required_capabilities",schemaPath:"#/properties/items/items/properties/required_capabilities/minItems",keyword:"minItems",params:{limit: 0},message:"must NOT have fewer than 0 items"}];
return false;
}
else {
var valid7 = true;
const len2 = data12.length;
for(let i2=0; i2<len2; i2++){
let data13 = data12[i2];
const _errs33 = errors;
if(errors === _errs33){
if(typeof data13 === "string"){
if(func2(data13) > 120){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/required_capabilities/" + i2,schemaPath:"#/properties/items/items/properties/required_capabilities/items/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data13) < 1){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/required_capabilities/" + i2,schemaPath:"#/properties/items/items/properties/required_capabilities/items/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/required_capabilities/" + i2,schemaPath:"#/properties/items/items/properties/required_capabilities/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid7 = _errs33 === errors;
if(!valid7){
break;
}
}
if(valid7){
let i3 = data12.length;
let j0;
if(i3 > 1){
const indices0 = {};
for(;i3--;){
let item0 = data12[i3];
if(typeof item0 !== "string"){
continue;
}
if(typeof indices0[item0] == "number"){
j0 = indices0[item0];
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/required_capabilities",schemaPath:"#/properties/items/items/properties/required_capabilities/uniqueItems",keyword:"uniqueItems",params:{i: i3, j: j0},message:"must NOT have duplicate items (items ## "+j0+" and "+i3+" are identical)"}];
return false;
break;
}
indices0[item0] = i3;
}
}
}
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/required_capabilities",schemaPath:"#/properties/items/items/properties/required_capabilities/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid2 = _errs31 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.selection_rule !== undefined){
const _errs35 = errors;
if(!(validate75(data1.selection_rule, {instancePath:instancePath+"/items/" + i0+"/selection_rule",parentData:data1,parentDataProperty:"selection_rule",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate75.errors : vErrors.concat(validate75.errors);
errors = vErrors.length;
}
var valid2 = _errs35 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.split_policy !== undefined){
const _errs36 = errors;
if(!(validate77(data1.split_policy, {instancePath:instancePath+"/items/" + i0+"/split_policy",parentData:data1,parentDataProperty:"split_policy",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
errors = vErrors.length;
}
var valid2 = _errs36 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.validity_seconds !== undefined){
let data16 = data1.validity_seconds;
const _errs37 = errors;
const _errs38 = errors;
if(errors === _errs38){
if(typeof data16 === "string"){
if(func2(data16) > 19){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data16) < 1){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data16)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs37 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.version !== undefined){
let data17 = data1.version;
const _errs40 = errors;
if(!((typeof data17 == "number") && (!(data17 % 1) && !isNaN(data17)))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs40){
if(typeof data17 == "number"){
if(data17 > 2147483647 || isNaN(data17)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data17 < 1 || isNaN(data17)){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data17))){
validate72.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid2 = _errs40 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate72.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data18 = data.next_cursor;
const _errs42 = errors;
const _errs43 = errors;
let valid10 = false;
let passing0 = null;
const _errs44 = errors;
if(data18 !== null){
const err0 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs44 === errors;
if(_valid0){
valid10 = true;
passing0 = 0;
}
const _errs46 = errors;
const _errs47 = errors;
if(errors === _errs47){
if(errors === _errs47){
if(typeof data18 === "string"){
if(!pattern5.test(data18)){
const err1 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data18))){
const err2 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs46 === errors;
if(_valid0 && valid10){
valid10 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid10 = true;
passing0 = 1;
}
}
if(!valid10){
const err4 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate72.errors = vErrors;
return false;
}
else {
errors = _errs43;
if(vErrors !== null){
if(_errs43){
vErrors.length = _errs43;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs42 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data19 = data.schema_version;
const _errs49 = errors;
const _errs50 = errors;
if(!((typeof data19 == "number") && (!(data19 % 1) && !isNaN(data19)))){
validate72.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data19 === 1)){
validate72.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs50){
if(typeof data19 == "number"){
if(data19 > 1 || isNaN(data19)){
validate72.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data19 < 1 || isNaN(data19)){
validate72.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs49 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate72.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate72.errors = vErrors;
return errors === 0;
}
validate72.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate71(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate71.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate72(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate72.errors : vErrors.concat(validate72.errors);
errors = vErrors.length;
}
validate71.errors = vErrors;
return errors === 0;
}
validate71.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response15 = validate80;
const schema177 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1evaluation-policies/post/responses/201/content/application~1json/schema"};
const schema178 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"created_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"maximum_missing_fraction":{"allOf":[{"description":"Plain decimal exactly representable by NUMERIC(38,18).","maxLength":64,"minLength":1,"pattern":"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])","type":"string"},{"pattern":"^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])","type":"string"}]},"maximum_sealed_uses_per_lineage":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"},"metric_requirements":{"items":{"$ref":"#/components/schemas/MetricRequirementV1"},"maxItems":64,"minItems":1,"type":"array"},"minimum_observations":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"},"project_id":{"$ref":"#/components/schemas/Id"},"question":{"maxLength":8000,"minLength":1,"type":"string"},"require_real_data":{"type":"boolean"},"required_capabilities":{"items":{"maxLength":120,"minLength":1,"type":"string"},"maxItems":64,"minItems":0,"type":"array","uniqueItems":true},"selection_rule":{"$ref":"#/components/schemas/SelectionRuleV1"},"split_policy":{"$ref":"#/components/schemas/SplitPolicyV1"},"validity_seconds":{"$ref":"#/components/schemas/DbCounter"},"version":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"}},"required":["id","project_id","version","created_at","question","selection_rule","split_policy","metric_requirements","minimum_observations","maximum_missing_fraction","require_real_data","required_capabilities","maximum_sealed_uses_per_lineage","validity_seconds"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};

function validate81(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate81.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate81.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate81.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate81.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if(((((((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.version === undefined) && (missing1 = "version"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.question === undefined) && (missing1 = "question"))) || ((data1.selection_rule === undefined) && (missing1 = "selection_rule"))) || ((data1.split_policy === undefined) && (missing1 = "split_policy"))) || ((data1.metric_requirements === undefined) && (missing1 = "metric_requirements"))) || ((data1.minimum_observations === undefined) && (missing1 = "minimum_observations"))) || ((data1.maximum_missing_fraction === undefined) && (missing1 = "maximum_missing_fraction"))) || ((data1.require_real_data === undefined) && (missing1 = "require_real_data"))) || ((data1.required_capabilities === undefined) && (missing1 = "required_capabilities"))) || ((data1.maximum_sealed_uses_per_lineage === undefined) && (missing1 = "maximum_sealed_uses_per_lineage"))) || ((data1.validity_seconds === undefined) && (missing1 = "validity_seconds"))){
validate81.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema178.properties.resource.properties, key1))){
validate81.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.created_at !== undefined){
let data2 = data1.created_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate81.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data3 = data1.id;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
validate81.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data3))){
validate81.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs9 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.maximum_missing_fraction !== undefined){
let data4 = data1.maximum_missing_fraction;
const _errs12 = errors;
const _errs13 = errors;
if(errors === _errs13){
if(typeof data4 === "string"){
if(func2(data4) > 64){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_missing_fraction",schemaPath:"#/properties/resource/properties/maximum_missing_fraction/allOf/0/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"}];
return false;
}
else {
if(func2(data4) < 1){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_missing_fraction",schemaPath:"#/properties/resource/properties/maximum_missing_fraction/allOf/0/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern38.test(data4)){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_missing_fraction",schemaPath:"#/properties/resource/properties/maximum_missing_fraction/allOf/0/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/maximum_missing_fraction",schemaPath:"#/properties/resource/properties/maximum_missing_fraction/allOf/0/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid3 = _errs13 === errors;
if(valid3){
const _errs15 = errors;
if(errors === _errs15){
if(typeof data4 === "string"){
if(!pattern75.test(data4)){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_missing_fraction",schemaPath:"#/properties/resource/properties/maximum_missing_fraction/allOf/1/pattern",keyword:"pattern",params:{pattern: "^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])"},message:"must match pattern \""+"^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])"+"\""}];
return false;
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/maximum_missing_fraction",schemaPath:"#/properties/resource/properties/maximum_missing_fraction/allOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid3 = _errs15 === errors;
}
var valid1 = _errs12 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.maximum_sealed_uses_per_lineage !== undefined){
let data5 = data1.maximum_sealed_uses_per_lineage;
const _errs17 = errors;
if(!((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5)))){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/resource/properties/maximum_sealed_uses_per_lineage/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs17){
if(typeof data5 == "number"){
if(data5 > 2147483647 || isNaN(data5)){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/resource/properties/maximum_sealed_uses_per_lineage/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data5 < 1 || isNaN(data5)){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/resource/properties/maximum_sealed_uses_per_lineage/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data5))){
validate81.errors = [{instancePath:instancePath+"/resource/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/resource/properties/maximum_sealed_uses_per_lineage/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid1 = _errs17 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.metric_requirements !== undefined){
let data6 = data1.metric_requirements;
const _errs19 = errors;
if(errors === _errs19){
if(Array.isArray(data6)){
if(data6.length > 64){
validate81.errors = [{instancePath:instancePath+"/resource/metric_requirements",schemaPath:"#/properties/resource/properties/metric_requirements/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data6.length < 1){
validate81.errors = [{instancePath:instancePath+"/resource/metric_requirements",schemaPath:"#/properties/resource/properties/metric_requirements/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid4 = true;
const len0 = data6.length;
for(let i0=0; i0<len0; i0++){
const _errs21 = errors;
if(!(validate73(data6[i0], {instancePath:instancePath+"/resource/metric_requirements/" + i0,parentData:data6,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate73.errors : vErrors.concat(validate73.errors);
errors = vErrors.length;
}
var valid4 = _errs21 === errors;
if(!valid4){
break;
}
}
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/metric_requirements",schemaPath:"#/properties/resource/properties/metric_requirements/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid1 = _errs19 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.minimum_observations !== undefined){
let data8 = data1.minimum_observations;
const _errs22 = errors;
if(!((typeof data8 == "number") && (!(data8 % 1) && !isNaN(data8)))){
validate81.errors = [{instancePath:instancePath+"/resource/minimum_observations",schemaPath:"#/properties/resource/properties/minimum_observations/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs22){
if(typeof data8 == "number"){
if(data8 > 2147483647 || isNaN(data8)){
validate81.errors = [{instancePath:instancePath+"/resource/minimum_observations",schemaPath:"#/properties/resource/properties/minimum_observations/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data8 < 1 || isNaN(data8)){
validate81.errors = [{instancePath:instancePath+"/resource/minimum_observations",schemaPath:"#/properties/resource/properties/minimum_observations/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data8))){
validate81.errors = [{instancePath:instancePath+"/resource/minimum_observations",schemaPath:"#/properties/resource/properties/minimum_observations/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid1 = _errs22 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.project_id !== undefined){
let data9 = data1.project_id;
const _errs24 = errors;
const _errs25 = errors;
if(errors === _errs25){
if(errors === _errs25){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
validate81.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data9))){
validate81.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs24 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.question !== undefined){
let data10 = data1.question;
const _errs27 = errors;
if(errors === _errs27){
if(typeof data10 === "string"){
if(func2(data10) > 8000){
validate81.errors = [{instancePath:instancePath+"/resource/question",schemaPath:"#/properties/resource/properties/question/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate81.errors = [{instancePath:instancePath+"/resource/question",schemaPath:"#/properties/resource/properties/question/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/question",schemaPath:"#/properties/resource/properties/question/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs27 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.require_real_data !== undefined){
const _errs29 = errors;
if(typeof data1.require_real_data !== "boolean"){
validate81.errors = [{instancePath:instancePath+"/resource/require_real_data",schemaPath:"#/properties/resource/properties/require_real_data/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid1 = _errs29 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.required_capabilities !== undefined){
let data12 = data1.required_capabilities;
const _errs31 = errors;
if(errors === _errs31){
if(Array.isArray(data12)){
if(data12.length > 64){
validate81.errors = [{instancePath:instancePath+"/resource/required_capabilities",schemaPath:"#/properties/resource/properties/required_capabilities/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data12.length < 0){
validate81.errors = [{instancePath:instancePath+"/resource/required_capabilities",schemaPath:"#/properties/resource/properties/required_capabilities/minItems",keyword:"minItems",params:{limit: 0},message:"must NOT have fewer than 0 items"}];
return false;
}
else {
var valid6 = true;
const len1 = data12.length;
for(let i1=0; i1<len1; i1++){
let data13 = data12[i1];
const _errs33 = errors;
if(errors === _errs33){
if(typeof data13 === "string"){
if(func2(data13) > 120){
validate81.errors = [{instancePath:instancePath+"/resource/required_capabilities/" + i1,schemaPath:"#/properties/resource/properties/required_capabilities/items/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data13) < 1){
validate81.errors = [{instancePath:instancePath+"/resource/required_capabilities/" + i1,schemaPath:"#/properties/resource/properties/required_capabilities/items/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/required_capabilities/" + i1,schemaPath:"#/properties/resource/properties/required_capabilities/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid6 = _errs33 === errors;
if(!valid6){
break;
}
}
if(valid6){
let i2 = data12.length;
let j0;
if(i2 > 1){
const indices0 = {};
for(;i2--;){
let item0 = data12[i2];
if(typeof item0 !== "string"){
continue;
}
if(typeof indices0[item0] == "number"){
j0 = indices0[item0];
validate81.errors = [{instancePath:instancePath+"/resource/required_capabilities",schemaPath:"#/properties/resource/properties/required_capabilities/uniqueItems",keyword:"uniqueItems",params:{i: i2, j: j0},message:"must NOT have duplicate items (items ## "+j0+" and "+i2+" are identical)"}];
return false;
break;
}
indices0[item0] = i2;
}
}
}
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/required_capabilities",schemaPath:"#/properties/resource/properties/required_capabilities/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid1 = _errs31 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.selection_rule !== undefined){
const _errs35 = errors;
if(!(validate75(data1.selection_rule, {instancePath:instancePath+"/resource/selection_rule",parentData:data1,parentDataProperty:"selection_rule",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate75.errors : vErrors.concat(validate75.errors);
errors = vErrors.length;
}
var valid1 = _errs35 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.split_policy !== undefined){
const _errs36 = errors;
if(!(validate77(data1.split_policy, {instancePath:instancePath+"/resource/split_policy",parentData:data1,parentDataProperty:"split_policy",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
errors = vErrors.length;
}
var valid1 = _errs36 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.validity_seconds !== undefined){
let data16 = data1.validity_seconds;
const _errs37 = errors;
const _errs38 = errors;
if(errors === _errs38){
if(typeof data16 === "string"){
if(func2(data16) > 19){
validate81.errors = [{instancePath:instancePath+"/resource/validity_seconds",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data16) < 1){
validate81.errors = [{instancePath:instancePath+"/resource/validity_seconds",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data16)){
validate81.errors = [{instancePath:instancePath+"/resource/validity_seconds",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource/validity_seconds",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs37 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.version !== undefined){
let data17 = data1.version;
const _errs40 = errors;
if(!((typeof data17 == "number") && (!(data17 % 1) && !isNaN(data17)))){
validate81.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs40){
if(typeof data17 == "number"){
if(data17 > 2147483647 || isNaN(data17)){
validate81.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data17 < 1 || isNaN(data17)){
validate81.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data17))){
validate81.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid1 = _errs40 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate81.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data18 = data.schema_version;
const _errs42 = errors;
const _errs43 = errors;
if(!((typeof data18 == "number") && (!(data18 % 1) && !isNaN(data18)))){
validate81.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data18 === 1)){
validate81.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs43){
if(typeof data18 == "number"){
if(data18 > 1 || isNaN(data18)){
validate81.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data18 < 1 || isNaN(data18)){
validate81.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs42 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate81.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate81.errors = vErrors;
return errors === 0;
}
validate81.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate80(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate80.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate81(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate81.errors : vErrors.concat(validate81.errors);
errors = vErrors.length;
}
validate80.errors = vErrors;
return errors === 0;
}
validate80.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response16 = validate86;
const schema183 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1evaluation-policies~1{id}/get/responses/200/content/application~1json/schema"};
const schema184 = {"additionalProperties":false,"properties":{"created_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"maximum_missing_fraction":{"allOf":[{"description":"Plain decimal exactly representable by NUMERIC(38,18).","maxLength":64,"minLength":1,"pattern":"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])","type":"string"},{"pattern":"^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])","type":"string"}]},"maximum_sealed_uses_per_lineage":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"},"metric_requirements":{"items":{"$ref":"#/components/schemas/MetricRequirementV1"},"maxItems":64,"minItems":1,"type":"array"},"minimum_observations":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"},"project_id":{"$ref":"#/components/schemas/Id"},"question":{"maxLength":8000,"minLength":1,"type":"string"},"require_real_data":{"type":"boolean"},"required_capabilities":{"items":{"maxLength":120,"minLength":1,"type":"string"},"maxItems":64,"minItems":0,"type":"array","uniqueItems":true},"selection_rule":{"$ref":"#/components/schemas/SelectionRuleV1"},"split_policy":{"$ref":"#/components/schemas/SplitPolicyV1"},"validity_seconds":{"$ref":"#/components/schemas/DbCounter"},"version":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"}},"required":["id","project_id","version","created_at","question","selection_rule","split_policy","metric_requirements","minimum_observations","maximum_missing_fraction","require_real_data","required_capabilities","maximum_sealed_uses_per_lineage","validity_seconds"],"type":"object"};

function validate87(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate87.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((((((((((((data.id === undefined) && (missing0 = "id")) || ((data.project_id === undefined) && (missing0 = "project_id"))) || ((data.version === undefined) && (missing0 = "version"))) || ((data.created_at === undefined) && (missing0 = "created_at"))) || ((data.question === undefined) && (missing0 = "question"))) || ((data.selection_rule === undefined) && (missing0 = "selection_rule"))) || ((data.split_policy === undefined) && (missing0 = "split_policy"))) || ((data.metric_requirements === undefined) && (missing0 = "metric_requirements"))) || ((data.minimum_observations === undefined) && (missing0 = "minimum_observations"))) || ((data.maximum_missing_fraction === undefined) && (missing0 = "maximum_missing_fraction"))) || ((data.require_real_data === undefined) && (missing0 = "require_real_data"))) || ((data.required_capabilities === undefined) && (missing0 = "required_capabilities"))) || ((data.maximum_sealed_uses_per_lineage === undefined) && (missing0 = "maximum_sealed_uses_per_lineage"))) || ((data.validity_seconds === undefined) && (missing0 = "validity_seconds"))){
validate87.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema184.properties, key0))){
validate87.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.created_at !== undefined){
let data0 = data.created_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate87.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate87.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data1 = data.id;
const _errs4 = errors;
const _errs5 = errors;
if(errors === _errs5){
if(errors === _errs5){
if(typeof data1 === "string"){
if(!pattern5.test(data1)){
validate87.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data1))){
validate87.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.maximum_missing_fraction !== undefined){
let data2 = data.maximum_missing_fraction;
const _errs7 = errors;
const _errs8 = errors;
if(errors === _errs8){
if(typeof data2 === "string"){
if(func2(data2) > 64){
validate87.errors = [{instancePath:instancePath+"/maximum_missing_fraction",schemaPath:"#/properties/maximum_missing_fraction/allOf/0/maxLength",keyword:"maxLength",params:{limit: 64},message:"must NOT have more than 64 characters"}];
return false;
}
else {
if(func2(data2) < 1){
validate87.errors = [{instancePath:instancePath+"/maximum_missing_fraction",schemaPath:"#/properties/maximum_missing_fraction/allOf/0/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern38.test(data2)){
validate87.errors = [{instancePath:instancePath+"/maximum_missing_fraction",schemaPath:"#/properties/maximum_missing_fraction/allOf/0/pattern",keyword:"pattern",params:{pattern: "^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"},message:"must match pattern \""+"^[+-]?(?:0*[0-9]{1,20}(?:\\.[0-9]{0,18}0*)?|\\.[0-9]{1,18}0*)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/maximum_missing_fraction",schemaPath:"#/properties/maximum_missing_fraction/allOf/0/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs8 === errors;
if(valid2){
const _errs10 = errors;
if(errors === _errs10){
if(typeof data2 === "string"){
if(!pattern75.test(data2)){
validate87.errors = [{instancePath:instancePath+"/maximum_missing_fraction",schemaPath:"#/properties/maximum_missing_fraction/allOf/1/pattern",keyword:"pattern",params:{pattern: "^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])"},message:"must match pattern \""+"^(?:\\+?(?:0*1(?:\\.0*)?|0+(?:\\.[0-9]*)?|\\.[0-9]+)|-(?:0+(?:\\.0*)?|\\.0+))(?![\\s\\S])"+"\""}];
return false;
}
}
else {
validate87.errors = [{instancePath:instancePath+"/maximum_missing_fraction",schemaPath:"#/properties/maximum_missing_fraction/allOf/1/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs10 === errors;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.maximum_sealed_uses_per_lineage !== undefined){
let data3 = data.maximum_sealed_uses_per_lineage;
const _errs12 = errors;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
validate87.errors = [{instancePath:instancePath+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/maximum_sealed_uses_per_lineage/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs12){
if(typeof data3 == "number"){
if(data3 > 2147483647 || isNaN(data3)){
validate87.errors = [{instancePath:instancePath+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/maximum_sealed_uses_per_lineage/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data3 < 1 || isNaN(data3)){
validate87.errors = [{instancePath:instancePath+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/maximum_sealed_uses_per_lineage/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data3))){
validate87.errors = [{instancePath:instancePath+"/maximum_sealed_uses_per_lineage",schemaPath:"#/properties/maximum_sealed_uses_per_lineage/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs12 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.metric_requirements !== undefined){
let data4 = data.metric_requirements;
const _errs14 = errors;
if(errors === _errs14){
if(Array.isArray(data4)){
if(data4.length > 64){
validate87.errors = [{instancePath:instancePath+"/metric_requirements",schemaPath:"#/properties/metric_requirements/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data4.length < 1){
validate87.errors = [{instancePath:instancePath+"/metric_requirements",schemaPath:"#/properties/metric_requirements/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid3 = true;
const len0 = data4.length;
for(let i0=0; i0<len0; i0++){
const _errs16 = errors;
if(!(validate73(data4[i0], {instancePath:instancePath+"/metric_requirements/" + i0,parentData:data4,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate73.errors : vErrors.concat(validate73.errors);
errors = vErrors.length;
}
var valid3 = _errs16 === errors;
if(!valid3){
break;
}
}
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/metric_requirements",schemaPath:"#/properties/metric_requirements/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs14 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.minimum_observations !== undefined){
let data6 = data.minimum_observations;
const _errs17 = errors;
if(!((typeof data6 == "number") && (!(data6 % 1) && !isNaN(data6)))){
validate87.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/properties/minimum_observations/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs17){
if(typeof data6 == "number"){
if(data6 > 2147483647 || isNaN(data6)){
validate87.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/properties/minimum_observations/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data6 < 1 || isNaN(data6)){
validate87.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/properties/minimum_observations/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data6))){
validate87.errors = [{instancePath:instancePath+"/minimum_observations",schemaPath:"#/properties/minimum_observations/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs17 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.project_id !== undefined){
let data7 = data.project_id;
const _errs19 = errors;
const _errs20 = errors;
if(errors === _errs20){
if(errors === _errs20){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate87.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate87.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs19 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.question !== undefined){
let data8 = data.question;
const _errs22 = errors;
if(errors === _errs22){
if(typeof data8 === "string"){
if(func2(data8) > 8000){
validate87.errors = [{instancePath:instancePath+"/question",schemaPath:"#/properties/question/maxLength",keyword:"maxLength",params:{limit: 8000},message:"must NOT have more than 8000 characters"}];
return false;
}
else {
if(func2(data8) < 1){
validate87.errors = [{instancePath:instancePath+"/question",schemaPath:"#/properties/question/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/question",schemaPath:"#/properties/question/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs22 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.require_real_data !== undefined){
const _errs24 = errors;
if(typeof data.require_real_data !== "boolean"){
validate87.errors = [{instancePath:instancePath+"/require_real_data",schemaPath:"#/properties/require_real_data/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs24 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.required_capabilities !== undefined){
let data10 = data.required_capabilities;
const _errs26 = errors;
if(errors === _errs26){
if(Array.isArray(data10)){
if(data10.length > 64){
validate87.errors = [{instancePath:instancePath+"/required_capabilities",schemaPath:"#/properties/required_capabilities/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data10.length < 0){
validate87.errors = [{instancePath:instancePath+"/required_capabilities",schemaPath:"#/properties/required_capabilities/minItems",keyword:"minItems",params:{limit: 0},message:"must NOT have fewer than 0 items"}];
return false;
}
else {
var valid5 = true;
const len1 = data10.length;
for(let i1=0; i1<len1; i1++){
let data11 = data10[i1];
const _errs28 = errors;
if(errors === _errs28){
if(typeof data11 === "string"){
if(func2(data11) > 120){
validate87.errors = [{instancePath:instancePath+"/required_capabilities/" + i1,schemaPath:"#/properties/required_capabilities/items/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func2(data11) < 1){
validate87.errors = [{instancePath:instancePath+"/required_capabilities/" + i1,schemaPath:"#/properties/required_capabilities/items/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/required_capabilities/" + i1,schemaPath:"#/properties/required_capabilities/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid5 = _errs28 === errors;
if(!valid5){
break;
}
}
if(valid5){
let i2 = data10.length;
let j0;
if(i2 > 1){
const indices0 = {};
for(;i2--;){
let item0 = data10[i2];
if(typeof item0 !== "string"){
continue;
}
if(typeof indices0[item0] == "number"){
j0 = indices0[item0];
validate87.errors = [{instancePath:instancePath+"/required_capabilities",schemaPath:"#/properties/required_capabilities/uniqueItems",keyword:"uniqueItems",params:{i: i2, j: j0},message:"must NOT have duplicate items (items ## "+j0+" and "+i2+" are identical)"}];
return false;
break;
}
indices0[item0] = i2;
}
}
}
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/required_capabilities",schemaPath:"#/properties/required_capabilities/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs26 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.selection_rule !== undefined){
const _errs30 = errors;
if(!(validate75(data.selection_rule, {instancePath:instancePath+"/selection_rule",parentData:data,parentDataProperty:"selection_rule",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate75.errors : vErrors.concat(validate75.errors);
errors = vErrors.length;
}
var valid0 = _errs30 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.split_policy !== undefined){
const _errs31 = errors;
if(!(validate77(data.split_policy, {instancePath:instancePath+"/split_policy",parentData:data,parentDataProperty:"split_policy",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate77.errors : vErrors.concat(validate77.errors);
errors = vErrors.length;
}
var valid0 = _errs31 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.validity_seconds !== undefined){
let data14 = data.validity_seconds;
const _errs32 = errors;
const _errs33 = errors;
if(errors === _errs33){
if(typeof data14 === "string"){
if(func2(data14) > 19){
validate87.errors = [{instancePath:instancePath+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data14) < 1){
validate87.errors = [{instancePath:instancePath+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data14)){
validate87.errors = [{instancePath:instancePath+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate87.errors = [{instancePath:instancePath+"/validity_seconds",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs32 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.version !== undefined){
let data15 = data.version;
const _errs35 = errors;
if(!((typeof data15 == "number") && (!(data15 % 1) && !isNaN(data15)))){
validate87.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs35){
if(typeof data15 == "number"){
if(data15 > 2147483647 || isNaN(data15)){
validate87.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data15 < 1 || isNaN(data15)){
validate87.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data15))){
validate87.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs35 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate87.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate87.errors = vErrors;
return errors === 0;
}
validate87.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate86(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate86.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate87(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate87.errors : vErrors.concat(validate87.errors);
errors = vErrors.length;
}
validate86.errors = vErrors;
return errors === 0;
}
validate86.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response17 = validate92;
const schema188 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1input-sets/get/responses/200/content/application~1json/schema"};
const schema189 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"created_at":{"format":"date-time","type":"string"},"decision_cutoff":{"format":"date-time","type":"string"},"frozen_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"project_id":{"$ref":"#/components/schemas/Id"},"purpose":{"$ref":"#/components/schemas/InputPurpose"},"revision":{"$ref":"#/components/schemas/Revision"}},"required":["id","project_id","purpose","decision_cutoff","frozen_at","revision","created_at"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};
const schema192 = {"enum":["DISCOVERY","VALIDATION","SEALED","PORTFOLIO","FORWARD"],"type":"string"};

function validate93(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate93.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate93.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate93.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.purpose === undefined) && (missing1 = "purpose"))) || ((data1.decision_cutoff === undefined) && (missing1 = "decision_cutoff"))) || ((data1.frozen_at === undefined) && (missing1 = "frozen_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(((((((key1 === "created_at") || (key1 === "decision_cutoff")) || (key1 === "frozen_at")) || (key1 === "id")) || (key1 === "project_id")) || (key1 === "purpose")) || (key1 === "revision"))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.created_at !== undefined){
let data2 = data1.created_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.decision_cutoff !== undefined){
let data3 = data1.decision_cutoff;
const _errs9 = errors;
if(errors === _errs9){
if(errors === _errs9){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/decision_cutoff",schemaPath:"#/properties/items/items/properties/decision_cutoff/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/decision_cutoff",schemaPath:"#/properties/items/items/properties/decision_cutoff/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs9 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.frozen_at !== undefined){
let data4 = data1.frozen_at;
const _errs11 = errors;
if(errors === _errs11){
if(errors === _errs11){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/frozen_at",schemaPath:"#/properties/items/items/properties/frozen_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/frozen_at",schemaPath:"#/properties/items/items/properties/frozen_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs11 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data5 = data1.id;
const _errs13 = errors;
const _errs14 = errors;
if(errors === _errs14){
if(errors === _errs14){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data5))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs13 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.project_id !== undefined){
let data6 = data1.project_id;
const _errs16 = errors;
const _errs17 = errors;
if(errors === _errs17){
if(errors === _errs17){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs16 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.purpose !== undefined){
let data7 = data1.purpose;
const _errs19 = errors;
if(typeof data7 !== "string"){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/purpose",schemaPath:"#/components/schemas/InputPurpose/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(((((data7 === "DISCOVERY") || (data7 === "VALIDATION")) || (data7 === "SEALED")) || (data7 === "PORTFOLIO")) || (data7 === "FORWARD"))){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/purpose",schemaPath:"#/components/schemas/InputPurpose/enum",keyword:"enum",params:{allowedValues: schema192.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs19 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.revision !== undefined){
let data8 = data1.revision;
const _errs22 = errors;
const _errs23 = errors;
if(errors === _errs23){
if(typeof data8 === "string"){
if(func2(data8) > 19){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data8) < 1){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data8)){
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs22 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate93.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data9 = data.next_cursor;
const _errs25 = errors;
const _errs26 = errors;
let valid7 = false;
let passing0 = null;
const _errs27 = errors;
if(data9 !== null){
const err0 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs27 === errors;
if(_valid0){
valid7 = true;
passing0 = 0;
}
const _errs29 = errors;
const _errs30 = errors;
if(errors === _errs30){
if(errors === _errs30){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
const err1 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data9))){
const err2 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs29 === errors;
if(_valid0 && valid7){
valid7 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid7 = true;
passing0 = 1;
}
}
if(!valid7){
const err4 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate93.errors = vErrors;
return false;
}
else {
errors = _errs26;
if(vErrors !== null){
if(_errs26){
vErrors.length = _errs26;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs25 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data10 = data.schema_version;
const _errs32 = errors;
const _errs33 = errors;
if(!((typeof data10 == "number") && (!(data10 % 1) && !isNaN(data10)))){
validate93.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data10 === 1)){
validate93.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs33){
if(typeof data10 == "number"){
if(data10 > 1 || isNaN(data10)){
validate93.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data10 < 1 || isNaN(data10)){
validate93.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs32 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate93.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate93.errors = vErrors;
return errors === 0;
}
validate93.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate92(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate92.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate93(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate93.errors : vErrors.concat(validate93.errors);
errors = vErrors.length;
}
validate92.errors = vErrors;
return errors === 0;
}
validate92.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response18 = validate95;
const schema196 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1input-sets/post/responses/201/content/application~1json/schema"};
const schema197 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"header":{"$ref":"#/components/schemas/InputSetSummary"},"items":{"items":{"$ref":"#/components/schemas/InputItemView"},"maxItems":256,"minItems":1,"type":"array"}},"required":["header","items"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};
const schema198 = {"additionalProperties":false,"properties":{"created_at":{"format":"date-time","type":"string"},"decision_cutoff":{"format":"date-time","type":"string"},"frozen_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"project_id":{"$ref":"#/components/schemas/Id"},"purpose":{"$ref":"#/components/schemas/InputPurpose"},"revision":{"$ref":"#/components/schemas/Revision"}},"required":["id","project_id","purpose","decision_cutoff","frozen_at","revision","created_at"],"type":"object"};

function validate97(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate97.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((data.id === undefined) && (missing0 = "id")) || ((data.project_id === undefined) && (missing0 = "project_id"))) || ((data.purpose === undefined) && (missing0 = "purpose"))) || ((data.decision_cutoff === undefined) && (missing0 = "decision_cutoff"))) || ((data.frozen_at === undefined) && (missing0 = "frozen_at"))) || ((data.revision === undefined) && (missing0 = "revision"))) || ((data.created_at === undefined) && (missing0 = "created_at"))){
validate97.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((((key0 === "created_at") || (key0 === "decision_cutoff")) || (key0 === "frozen_at")) || (key0 === "id")) || (key0 === "project_id")) || (key0 === "purpose")) || (key0 === "revision"))){
validate97.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.created_at !== undefined){
let data0 = data.created_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate97.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate97.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.decision_cutoff !== undefined){
let data1 = data.decision_cutoff;
const _errs4 = errors;
if(errors === _errs4){
if(errors === _errs4){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate97.errors = [{instancePath:instancePath+"/decision_cutoff",schemaPath:"#/properties/decision_cutoff/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate97.errors = [{instancePath:instancePath+"/decision_cutoff",schemaPath:"#/properties/decision_cutoff/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.frozen_at !== undefined){
let data2 = data.frozen_at;
const _errs6 = errors;
if(errors === _errs6){
if(errors === _errs6){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate97.errors = [{instancePath:instancePath+"/frozen_at",schemaPath:"#/properties/frozen_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate97.errors = [{instancePath:instancePath+"/frozen_at",schemaPath:"#/properties/frozen_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data3 = data.id;
const _errs8 = errors;
const _errs9 = errors;
if(errors === _errs9){
if(errors === _errs9){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
validate97.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data3))){
validate97.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate97.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.project_id !== undefined){
let data4 = data.project_id;
const _errs11 = errors;
const _errs12 = errors;
if(errors === _errs12){
if(errors === _errs12){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
validate97.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data4))){
validate97.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate97.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.purpose !== undefined){
let data5 = data.purpose;
const _errs14 = errors;
if(typeof data5 !== "string"){
validate97.errors = [{instancePath:instancePath+"/purpose",schemaPath:"#/components/schemas/InputPurpose/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(((((data5 === "DISCOVERY") || (data5 === "VALIDATION")) || (data5 === "SEALED")) || (data5 === "PORTFOLIO")) || (data5 === "FORWARD"))){
validate97.errors = [{instancePath:instancePath+"/purpose",schemaPath:"#/components/schemas/InputPurpose/enum",keyword:"enum",params:{allowedValues: schema192.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs14 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revision !== undefined){
let data6 = data.revision;
const _errs17 = errors;
const _errs18 = errors;
if(errors === _errs18){
if(typeof data6 === "string"){
if(func2(data6) > 19){
validate97.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data6) < 1){
validate97.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data6)){
validate97.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate97.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs17 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
else {
validate97.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate97.errors = vErrors;
return errors === 0;
}
validate97.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

const schema203 = {"additionalProperties":false,"properties":{"id":{"$ref":"#/components/schemas/Id"},"item":{"$ref":"#/components/schemas/InputItemV1"},"ordinal":{"format":"int32","maximum":255,"minimum":0,"type":"integer"},"origin":{"$ref":"#/components/schemas/DataOrigin"},"pit_status":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/PitStatus"}]}},"required":["id","ordinal","item","origin"],"type":"object"};
const schema211 = {"enum":["VERIFIED","UNVERIFIED","INVALID"],"type":"string"};
const schema205 = {"oneOf":[{"properties":{"dataset_revision_id":{"$ref":"#/components/schemas/Id"},"kind":{"enum":["DATASET"],"type":"string"},"role":{"$ref":"#/components/schemas/DataPartition"}},"required":["dataset_revision_id","role","kind"],"type":"object"},{"properties":{"artifact_id":{"$ref":"#/components/schemas/Id"},"kind":{"enum":["ARTIFACT"],"type":"string"},"role":{"$ref":"#/components/schemas/ArtifactInputRole"}},"required":["artifact_id","role","kind"],"type":"object"}]};
const schema209 = {"enum":["CODE","PARAMETERS","SIGNALS","TARGETS","MODEL","REPORT","METRICS","DATA_QUALITY"],"type":"string"};

function validate100(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate100.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
const _errs0 = errors;
let valid0 = false;
let passing0 = null;
const _errs1 = errors;
if(errors === _errs1){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.dataset_revision_id === undefined) && (missing0 = "dataset_revision_id")) || ((data.role === undefined) && (missing0 = "role"))) || ((data.kind === undefined) && (missing0 = "kind"))){
const err0 = {instancePath,schemaPath:"#/oneOf/0/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
else {
if(data.dataset_revision_id !== undefined){
let data0 = data.dataset_revision_id;
const _errs3 = errors;
const _errs4 = errors;
if(errors === _errs4){
if(errors === _errs4){
if(typeof data0 === "string"){
if(!pattern5.test(data0)){
const err1 = {instancePath:instancePath+"/dataset_revision_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data0))){
const err2 = {instancePath:instancePath+"/dataset_revision_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/dataset_revision_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var valid1 = _errs3 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.kind !== undefined){
let data1 = data.kind;
const _errs6 = errors;
if(typeof data1 !== "string"){
const err4 = {instancePath:instancePath+"/kind",schemaPath:"#/oneOf/0/properties/kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
if(!(data1 === "DATASET")){
const err5 = {instancePath:instancePath+"/kind",schemaPath:"#/oneOf/0/properties/kind/enum",keyword:"enum",params:{allowedValues: schema205.oneOf[0].properties.kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var valid1 = _errs6 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data.role !== undefined){
let data2 = data.role;
const _errs8 = errors;
if(typeof data2 !== "string"){
const err6 = {instancePath:instancePath+"/role",schemaPath:"#/components/schemas/DataPartition/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
if(!((((data2 === "DISCOVERY") || (data2 === "VALIDATION")) || (data2 === "SEALED")) || (data2 === "FORWARD"))){
const err7 = {instancePath:instancePath+"/role",schemaPath:"#/components/schemas/DataPartition/enum",keyword:"enum",params:{allowedValues: schema116.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
var valid1 = _errs8 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
else {
const err8 = {instancePath,schemaPath:"#/oneOf/0/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
var _valid0 = _errs1 === errors;
if(_valid0){
valid0 = true;
passing0 = 0;
var props0 = {};
props0.dataset_revision_id = true;
props0.kind = true;
props0.role = true;
}
const _errs11 = errors;
if(errors === _errs11){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing1;
if((((data.artifact_id === undefined) && (missing1 = "artifact_id")) || ((data.role === undefined) && (missing1 = "role"))) || ((data.kind === undefined) && (missing1 = "kind"))){
const err9 = {instancePath,schemaPath:"#/oneOf/1/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
}
else {
if(data.artifact_id !== undefined){
let data3 = data.artifact_id;
const _errs13 = errors;
const _errs14 = errors;
if(errors === _errs14){
if(errors === _errs14){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
const err10 = {instancePath:instancePath+"/artifact_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
else {
if(!(formats2.test(data3))){
const err11 = {instancePath:instancePath+"/artifact_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
}
}
else {
const err12 = {instancePath:instancePath+"/artifact_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
var valid4 = _errs13 === errors;
}
else {
var valid4 = true;
}
if(valid4){
if(data.kind !== undefined){
let data4 = data.kind;
const _errs16 = errors;
if(typeof data4 !== "string"){
const err13 = {instancePath:instancePath+"/kind",schemaPath:"#/oneOf/1/properties/kind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
if(!(data4 === "ARTIFACT")){
const err14 = {instancePath:instancePath+"/kind",schemaPath:"#/oneOf/1/properties/kind/enum",keyword:"enum",params:{allowedValues: schema205.oneOf[1].properties.kind.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
}
var valid4 = _errs16 === errors;
}
else {
var valid4 = true;
}
if(valid4){
if(data.role !== undefined){
let data5 = data.role;
const _errs18 = errors;
if(typeof data5 !== "string"){
const err15 = {instancePath:instancePath+"/role",schemaPath:"#/components/schemas/ArtifactInputRole/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err15];
}
else {
vErrors.push(err15);
}
errors++;
}
if(!((((((((data5 === "CODE") || (data5 === "PARAMETERS")) || (data5 === "SIGNALS")) || (data5 === "TARGETS")) || (data5 === "MODEL")) || (data5 === "REPORT")) || (data5 === "METRICS")) || (data5 === "DATA_QUALITY"))){
const err16 = {instancePath:instancePath+"/role",schemaPath:"#/components/schemas/ArtifactInputRole/enum",keyword:"enum",params:{allowedValues: schema209.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err16];
}
else {
vErrors.push(err16);
}
errors++;
}
var valid4 = _errs18 === errors;
}
else {
var valid4 = true;
}
}
}
}
}
else {
const err17 = {instancePath,schemaPath:"#/oneOf/1/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err17];
}
else {
vErrors.push(err17);
}
errors++;
}
}
var _valid0 = _errs11 === errors;
if(_valid0 && valid0){
valid0 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid0 = true;
passing0 = 1;
if(props0 !== true){
props0 = props0 || {};
props0.artifact_id = true;
props0.kind = true;
props0.role = true;
}
}
}
if(!valid0){
const err18 = {instancePath,schemaPath:"#/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err18];
}
else {
vErrors.push(err18);
}
errors++;
validate100.errors = vErrors;
return false;
}
else {
errors = _errs0;
if(vErrors !== null){
if(_errs0){
vErrors.length = _errs0;
}
else {
vErrors = null;
}
}
}
validate100.errors = vErrors;
evaluated0.props = props0;
return errors === 0;
}
validate100.evaluated = {"dynamicProps":true,"dynamicItems":false};


function validate99(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate99.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.id === undefined) && (missing0 = "id")) || ((data.ordinal === undefined) && (missing0 = "ordinal"))) || ((data.item === undefined) && (missing0 = "item"))) || ((data.origin === undefined) && (missing0 = "origin"))){
validate99.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((key0 === "id") || (key0 === "item")) || (key0 === "ordinal")) || (key0 === "origin")) || (key0 === "pit_status"))){
validate99.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.id !== undefined){
let data0 = data.id;
const _errs2 = errors;
const _errs3 = errors;
if(errors === _errs3){
if(errors === _errs3){
if(typeof data0 === "string"){
if(!pattern5.test(data0)){
validate99.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data0))){
validate99.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate99.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.item !== undefined){
const _errs5 = errors;
if(!(validate100(data.item, {instancePath:instancePath+"/item",parentData:data,parentDataProperty:"item",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate100.errors : vErrors.concat(validate100.errors);
errors = vErrors.length;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.ordinal !== undefined){
let data2 = data.ordinal;
const _errs6 = errors;
if(!((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2)))){
validate99.errors = [{instancePath:instancePath+"/ordinal",schemaPath:"#/properties/ordinal/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs6){
if(typeof data2 == "number"){
if(data2 > 255 || isNaN(data2)){
validate99.errors = [{instancePath:instancePath+"/ordinal",schemaPath:"#/properties/ordinal/maximum",keyword:"maximum",params:{comparison: "<=", limit: 255},message:"must be <= 255"}];
return false;
}
else {
if(data2 < 0 || isNaN(data2)){
validate99.errors = [{instancePath:instancePath+"/ordinal",schemaPath:"#/properties/ordinal/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
else {
if(!(formats94.validate(data2))){
validate99.errors = [{instancePath:instancePath+"/ordinal",schemaPath:"#/properties/ordinal/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.origin !== undefined){
let data3 = data.origin;
const _errs8 = errors;
if(typeof data3 !== "string"){
validate99.errors = [{instancePath:instancePath+"/origin",schemaPath:"#/components/schemas/DataOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data3 === "REAL") || (data3 === "SYNTHETIC")) || (data3 === "FIXTURE")) || (data3 === "LEGACY_UNKNOWN"))){
validate99.errors = [{instancePath:instancePath+"/origin",schemaPath:"#/components/schemas/DataOrigin/enum",keyword:"enum",params:{allowedValues: schema38.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.pit_status !== undefined){
let data4 = data.pit_status;
const _errs11 = errors;
const _errs12 = errors;
let valid3 = false;
let passing0 = null;
const _errs13 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/pit_status",schemaPath:"#/properties/pit_status/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs13 === errors;
if(_valid0){
valid3 = true;
passing0 = 0;
}
const _errs15 = errors;
if(typeof data4 !== "string"){
const err1 = {instancePath:instancePath+"/pit_status",schemaPath:"#/components/schemas/PitStatus/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
if(!(((data4 === "VERIFIED") || (data4 === "UNVERIFIED")) || (data4 === "INVALID"))){
const err2 = {instancePath:instancePath+"/pit_status",schemaPath:"#/components/schemas/PitStatus/enum",keyword:"enum",params:{allowedValues: schema211.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
var _valid0 = _errs15 === errors;
if(_valid0 && valid3){
valid3 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid3 = true;
passing0 = 1;
}
}
if(!valid3){
const err3 = {instancePath:instancePath+"/pit_status",schemaPath:"#/properties/pit_status/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
validate99.errors = vErrors;
return false;
}
else {
errors = _errs12;
if(vErrors !== null){
if(_errs12){
vErrors.length = _errs12;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
else {
validate99.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate99.errors = vErrors;
return errors === 0;
}
validate99.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate96(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate96.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate96.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate96.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate96.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if(((data1.header === undefined) && (missing1 = "header")) || ((data1.items === undefined) && (missing1 = "items"))){
validate96.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!((key1 === "header") || (key1 === "items"))){
validate96.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.header !== undefined){
const _errs7 = errors;
if(!(validate97(data1.header, {instancePath:instancePath+"/resource/header",parentData:data1,parentDataProperty:"header",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
errors = vErrors.length;
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.items !== undefined){
let data3 = data1.items;
const _errs8 = errors;
if(errors === _errs8){
if(Array.isArray(data3)){
if(data3.length > 256){
validate96.errors = [{instancePath:instancePath+"/resource/items",schemaPath:"#/properties/resource/properties/items/maxItems",keyword:"maxItems",params:{limit: 256},message:"must NOT have more than 256 items"}];
return false;
}
else {
if(data3.length < 1){
validate96.errors = [{instancePath:instancePath+"/resource/items",schemaPath:"#/properties/resource/properties/items/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid2 = true;
const len0 = data3.length;
for(let i0=0; i0<len0; i0++){
const _errs10 = errors;
if(!(validate99(data3[i0], {instancePath:instancePath+"/resource/items/" + i0,parentData:data3,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate99.errors : vErrors.concat(validate99.errors);
errors = vErrors.length;
}
var valid2 = _errs10 === errors;
if(!valid2){
break;
}
}
}
}
}
else {
validate96.errors = [{instancePath:instancePath+"/resource/items",schemaPath:"#/properties/resource/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid1 = _errs8 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
else {
validate96.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data5 = data.schema_version;
const _errs11 = errors;
const _errs12 = errors;
if(!((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5)))){
validate96.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data5 === 1)){
validate96.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs12){
if(typeof data5 == "number"){
if(data5 > 1 || isNaN(data5)){
validate96.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data5 < 1 || isNaN(data5)){
validate96.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate96.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate96.errors = vErrors;
return errors === 0;
}
validate96.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate95(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate95.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate96(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate96.errors : vErrors.concat(validate96.errors);
errors = vErrors.length;
}
validate95.errors = vErrors;
return errors === 0;
}
validate95.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response19 = validate104;
const schema213 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1input-sets~1{id}/get/responses/200/content/application~1json/schema"};
const schema214 = {"additionalProperties":false,"properties":{"header":{"$ref":"#/components/schemas/InputSetSummary"},"items":{"items":{"$ref":"#/components/schemas/InputItemView"},"maxItems":256,"minItems":1,"type":"array"}},"required":["header","items"],"type":"object"};

function validate105(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate105.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.header === undefined) && (missing0 = "header")) || ((data.items === undefined) && (missing0 = "items"))){
validate105.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((key0 === "header") || (key0 === "items"))){
validate105.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.header !== undefined){
const _errs2 = errors;
if(!(validate97(data.header, {instancePath:instancePath+"/header",parentData:data,parentDataProperty:"header",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate97.errors : vErrors.concat(validate97.errors);
errors = vErrors.length;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.items !== undefined){
let data1 = data.items;
const _errs3 = errors;
if(errors === _errs3){
if(Array.isArray(data1)){
if(data1.length > 256){
validate105.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/maxItems",keyword:"maxItems",params:{limit: 256},message:"must NOT have more than 256 items"}];
return false;
}
else {
if(data1.length < 1){
validate105.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid1 = true;
const len0 = data1.length;
for(let i0=0; i0<len0; i0++){
const _errs5 = errors;
if(!(validate99(data1[i0], {instancePath:instancePath+"/items/" + i0,parentData:data1,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate99.errors : vErrors.concat(validate99.errors);
errors = vErrors.length;
}
var valid1 = _errs5 === errors;
if(!valid1){
break;
}
}
}
}
}
else {
validate105.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
else {
validate105.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate105.errors = vErrors;
return errors === 0;
}
validate105.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate104(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate104.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate105(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate105.errors : vErrors.concat(validate105.errors);
errors = vErrors.length;
}
validate104.errors = vErrors;
return errors === 0;
}
validate104.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response20 = validate109;
const schema215 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1machine-credentials~1{id}~1revoke/post/responses/200/content/application~1json/schema"};
const schema216 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"expires_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"issued_at":{"format":"date-time","type":"string"},"principal_epoch":{"$ref":"#/components/schemas/Revision"},"principal_id":{"$ref":"#/components/schemas/Id"},"public_token_id":{"$ref":"#/components/schemas/Id"},"revoked_at":{"format":"date-time","type":["string","null"]},"scope_codes":{"items":{"$ref":"#/components/schemas/MachineScope"},"type":"array"}},"required":["id","principal_id","public_token_id","principal_epoch","scope_codes","issued_at","expires_at"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};

function validate110(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate110.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate110.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate110.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate110.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.principal_id === undefined) && (missing1 = "principal_id"))) || ((data1.public_token_id === undefined) && (missing1 = "public_token_id"))) || ((data1.principal_epoch === undefined) && (missing1 = "principal_epoch"))) || ((data1.scope_codes === undefined) && (missing1 = "scope_codes"))) || ((data1.issued_at === undefined) && (missing1 = "issued_at"))) || ((data1.expires_at === undefined) && (missing1 = "expires_at"))){
validate110.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!((((((((key1 === "expires_at") || (key1 === "id")) || (key1 === "issued_at")) || (key1 === "principal_epoch")) || (key1 === "principal_id")) || (key1 === "public_token_id")) || (key1 === "revoked_at")) || (key1 === "scope_codes"))){
validate110.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.expires_at !== undefined){
let data2 = data1.expires_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate110.errors = [{instancePath:instancePath+"/resource/expires_at",schemaPath:"#/properties/resource/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource/expires_at",schemaPath:"#/properties/resource/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data3 = data1.id;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
validate110.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data3))){
validate110.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs9 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.issued_at !== undefined){
let data4 = data1.issued_at;
const _errs12 = errors;
if(errors === _errs12){
if(errors === _errs12){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate110.errors = [{instancePath:instancePath+"/resource/issued_at",schemaPath:"#/properties/resource/properties/issued_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource/issued_at",schemaPath:"#/properties/resource/properties/issued_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs12 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.principal_epoch !== undefined){
let data5 = data1.principal_epoch;
const _errs14 = errors;
const _errs15 = errors;
if(errors === _errs15){
if(typeof data5 === "string"){
if(func2(data5) > 19){
validate110.errors = [{instancePath:instancePath+"/resource/principal_epoch",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data5) < 1){
validate110.errors = [{instancePath:instancePath+"/resource/principal_epoch",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data5)){
validate110.errors = [{instancePath:instancePath+"/resource/principal_epoch",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource/principal_epoch",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs14 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.principal_id !== undefined){
let data6 = data1.principal_id;
const _errs17 = errors;
const _errs18 = errors;
if(errors === _errs18){
if(errors === _errs18){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate110.errors = [{instancePath:instancePath+"/resource/principal_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate110.errors = [{instancePath:instancePath+"/resource/principal_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource/principal_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs17 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.public_token_id !== undefined){
let data7 = data1.public_token_id;
const _errs20 = errors;
const _errs21 = errors;
if(errors === _errs21){
if(errors === _errs21){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate110.errors = [{instancePath:instancePath+"/resource/public_token_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate110.errors = [{instancePath:instancePath+"/resource/public_token_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource/public_token_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs20 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revoked_at !== undefined){
let data8 = data1.revoked_at;
const _errs23 = errors;
if((typeof data8 !== "string") && (data8 !== null)){
validate110.errors = [{instancePath:instancePath+"/resource/revoked_at",schemaPath:"#/properties/resource/properties/revoked_at/type",keyword:"type",params:{type: schema216.properties.resource.properties.revoked_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs23){
if(errors === _errs23){
if(typeof data8 === "string"){
if(!(formats0.validate(data8))){
validate110.errors = [{instancePath:instancePath+"/resource/revoked_at",schemaPath:"#/properties/resource/properties/revoked_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs23 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.scope_codes !== undefined){
let data9 = data1.scope_codes;
const _errs25 = errors;
if(errors === _errs25){
if(Array.isArray(data9)){
var valid6 = true;
const len0 = data9.length;
for(let i0=0; i0<len0; i0++){
let data10 = data9[i0];
const _errs27 = errors;
if(typeof data10 !== "string"){
validate110.errors = [{instancePath:instancePath+"/resource/scope_codes/" + i0,schemaPath:"#/components/schemas/MachineScope/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((((data10 === "RESEARCH_READ") || (data10 === "EXPERIMENT_SUBMIT")) || (data10 === "ARTIFACT_SUBMIT")) || (data10 === "EVIDENCE_READ")) || (data10 === "RUN_READ")) || (data10 === "RUN_CANCEL")) || (data10 === "DOWNSTREAM_CLAIM")) || (data10 === "DOWNSTREAM_ACK")) || (data10 === "FORWARD_SUBMIT")) || (data10 === "DOCTOR_READ"))){
validate110.errors = [{instancePath:instancePath+"/resource/scope_codes/" + i0,schemaPath:"#/components/schemas/MachineScope/enum",keyword:"enum",params:{allowedValues: schema83.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid6 = _errs27 === errors;
if(!valid6){
break;
}
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource/scope_codes",schemaPath:"#/properties/resource/properties/scope_codes/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid1 = _errs25 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
else {
validate110.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data11 = data.schema_version;
const _errs30 = errors;
const _errs31 = errors;
if(!((typeof data11 == "number") && (!(data11 % 1) && !isNaN(data11)))){
validate110.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data11 === 1)){
validate110.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs31){
if(typeof data11 == "number"){
if(data11 > 1 || isNaN(data11)){
validate110.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data11 < 1 || isNaN(data11)){
validate110.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs30 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate110.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate110.errors = vErrors;
return errors === 0;
}
validate110.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate109(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate109.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate110(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate110.errors : vErrors.concat(validate110.errors);
errors = vErrors.length;
}
validate109.errors = vErrors;
return errors === 0;
}
validate109.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response21 = validate112;
const schema223 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1machine-principals/get/responses/200/content/application~1json/schema"};
const schema224 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"created_at":{"format":"date-time","type":"string"},"credential_epoch":{"$ref":"#/components/schemas/Revision"},"downstream_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"enabled":{"type":"boolean"},"id":{"$ref":"#/components/schemas/Id"},"kind":{"$ref":"#/components/schemas/PrincipalKind"},"name":{"type":"string"},"project_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"revision":{"$ref":"#/components/schemas/Revision"},"run_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"updated_at":{"format":"date-time","type":"string"}},"required":["id","name","kind","enabled","credential_epoch","created_at","updated_at","revision"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};

function validate113(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate113.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate113.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate113.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if(((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.name === undefined) && (missing1 = "name"))) || ((data1.kind === undefined) && (missing1 = "kind"))) || ((data1.enabled === undefined) && (missing1 = "enabled"))) || ((data1.credential_epoch === undefined) && (missing1 = "credential_epoch"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate113.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema224.properties.items.items.properties, key1))){
validate113.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.created_at !== undefined){
let data2 = data1.created_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.credential_epoch !== undefined){
let data3 = data1.credential_epoch;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(typeof data3 === "string"){
if(func2(data3) > 19){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/credential_epoch",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data3) < 1){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/credential_epoch",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data3)){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/credential_epoch",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/credential_epoch",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs9 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.downstream_id !== undefined){
let data4 = data1.downstream_id;
const _errs12 = errors;
const _errs13 = errors;
let valid4 = false;
let passing0 = null;
const _errs14 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/items/" + i0+"/downstream_id",schemaPath:"#/properties/items/items/properties/downstream_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs14 === errors;
if(_valid0){
valid4 = true;
passing0 = 0;
}
const _errs16 = errors;
const _errs17 = errors;
if(errors === _errs17){
if(errors === _errs17){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err1 = {instancePath:instancePath+"/items/" + i0+"/downstream_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err2 = {instancePath:instancePath+"/items/" + i0+"/downstream_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/items/" + i0+"/downstream_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs16 === errors;
if(_valid0 && valid4){
valid4 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid4 = true;
passing0 = 1;
}
}
if(!valid4){
const err4 = {instancePath:instancePath+"/items/" + i0+"/downstream_id",schemaPath:"#/properties/items/items/properties/downstream_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate113.errors = vErrors;
return false;
}
else {
errors = _errs13;
if(vErrors !== null){
if(_errs13){
vErrors.length = _errs13;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs12 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.enabled !== undefined){
const _errs19 = errors;
if(typeof data1.enabled !== "boolean"){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/enabled",schemaPath:"#/properties/items/items/properties/enabled/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid2 = _errs19 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data6 = data1.id;
const _errs21 = errors;
const _errs22 = errors;
if(errors === _errs22){
if(errors === _errs22){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs21 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.kind !== undefined){
let data7 = data1.kind;
const _errs24 = errors;
if(typeof data7 !== "string"){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/kind",schemaPath:"#/components/schemas/PrincipalKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data7 === "CLI") || (data7 === "DOWNSTREAM")) || (data7 === "AUTOMATION")) || (data7 === "MISSION"))){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/kind",schemaPath:"#/components/schemas/PrincipalKind/enum",keyword:"enum",params:{allowedValues: schema79.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs24 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.name !== undefined){
const _errs27 = errors;
if(typeof data1.name !== "string"){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/name",schemaPath:"#/properties/items/items/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs27 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.project_id !== undefined){
let data9 = data1.project_id;
const _errs29 = errors;
const _errs30 = errors;
let valid8 = false;
let passing1 = null;
const _errs31 = errors;
if(data9 !== null){
const err5 = {instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/properties/items/items/properties/project_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs31 === errors;
if(_valid1){
valid8 = true;
passing1 = 0;
}
const _errs33 = errors;
const _errs34 = errors;
if(errors === _errs34){
if(errors === _errs34){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
const err6 = {instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data9))){
const err7 = {instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs33 === errors;
if(_valid1 && valid8){
valid8 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid8 = true;
passing1 = 1;
}
}
if(!valid8){
const err9 = {instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/properties/items/items/properties/project_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate113.errors = vErrors;
return false;
}
else {
errors = _errs30;
if(vErrors !== null){
if(_errs30){
vErrors.length = _errs30;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs29 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.revision !== undefined){
let data10 = data1.revision;
const _errs36 = errors;
const _errs37 = errors;
if(errors === _errs37){
if(typeof data10 === "string"){
if(func2(data10) > 19){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data10)){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs36 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.run_id !== undefined){
let data11 = data1.run_id;
const _errs39 = errors;
const _errs40 = errors;
let valid11 = false;
let passing2 = null;
const _errs41 = errors;
if(data11 !== null){
const err10 = {instancePath:instancePath+"/items/" + i0+"/run_id",schemaPath:"#/properties/items/items/properties/run_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
var _valid2 = _errs41 === errors;
if(_valid2){
valid11 = true;
passing2 = 0;
}
const _errs43 = errors;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err11 = {instancePath:instancePath+"/items/" + i0+"/run_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err12 = {instancePath:instancePath+"/items/" + i0+"/run_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
else {
const err13 = {instancePath:instancePath+"/items/" + i0+"/run_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
}
var _valid2 = _errs43 === errors;
if(_valid2 && valid11){
valid11 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid11 = true;
passing2 = 1;
}
}
if(!valid11){
const err14 = {instancePath:instancePath+"/items/" + i0+"/run_id",schemaPath:"#/properties/items/items/properties/run_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
validate113.errors = vErrors;
return false;
}
else {
errors = _errs40;
if(vErrors !== null){
if(_errs40){
vErrors.length = _errs40;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs39 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.updated_at !== undefined){
let data12 = data1.updated_at;
const _errs46 = errors;
if(errors === _errs46){
if(errors === _errs46){
if(typeof data12 === "string"){
if(!(formats0.validate(data12))){
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/updated_at",schemaPath:"#/properties/items/items/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate113.errors = [{instancePath:instancePath+"/items/" + i0+"/updated_at",schemaPath:"#/properties/items/items/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs46 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate113.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate113.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data13 = data.next_cursor;
const _errs48 = errors;
const _errs49 = errors;
let valid13 = false;
let passing3 = null;
const _errs50 = errors;
if(data13 !== null){
const err15 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err15];
}
else {
vErrors.push(err15);
}
errors++;
}
var _valid3 = _errs50 === errors;
if(_valid3){
valid13 = true;
passing3 = 0;
}
const _errs52 = errors;
const _errs53 = errors;
if(errors === _errs53){
if(errors === _errs53){
if(typeof data13 === "string"){
if(!pattern5.test(data13)){
const err16 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err16];
}
else {
vErrors.push(err16);
}
errors++;
}
else {
if(!(formats2.test(data13))){
const err17 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err17];
}
else {
vErrors.push(err17);
}
errors++;
}
}
}
else {
const err18 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err18];
}
else {
vErrors.push(err18);
}
errors++;
}
}
}
var _valid3 = _errs52 === errors;
if(_valid3 && valid13){
valid13 = false;
passing3 = [passing3, 1];
}
else {
if(_valid3){
valid13 = true;
passing3 = 1;
}
}
if(!valid13){
const err19 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing3},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err19];
}
else {
vErrors.push(err19);
}
errors++;
validate113.errors = vErrors;
return false;
}
else {
errors = _errs49;
if(vErrors !== null){
if(_errs49){
vErrors.length = _errs49;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs48 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data14 = data.schema_version;
const _errs55 = errors;
const _errs56 = errors;
if(!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))){
validate113.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data14 === 1)){
validate113.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs56){
if(typeof data14 == "number"){
if(data14 > 1 || isNaN(data14)){
validate113.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data14 < 1 || isNaN(data14)){
validate113.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs55 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate113.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate113.errors = vErrors;
return errors === 0;
}
validate113.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate112(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate112.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate113(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate113.errors : vErrors.concat(validate113.errors);
errors = vErrors.length;
}
validate112.errors = vErrors;
return errors === 0;
}
validate112.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response22 = validate115;
const schema234 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1machine-principals/post/responses/201/content/application~1json/schema"};
const schema235 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"created_at":{"format":"date-time","type":"string"},"credential_epoch":{"$ref":"#/components/schemas/Revision"},"downstream_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"enabled":{"type":"boolean"},"id":{"$ref":"#/components/schemas/Id"},"kind":{"$ref":"#/components/schemas/PrincipalKind"},"name":{"type":"string"},"project_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"revision":{"$ref":"#/components/schemas/Revision"},"run_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"updated_at":{"format":"date-time","type":"string"}},"required":["id","name","kind","enabled","credential_epoch","created_at","updated_at","revision"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};

function validate116(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate116.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate116.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate116.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate116.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if(((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.name === undefined) && (missing1 = "name"))) || ((data1.kind === undefined) && (missing1 = "kind"))) || ((data1.enabled === undefined) && (missing1 = "enabled"))) || ((data1.credential_epoch === undefined) && (missing1 = "credential_epoch"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate116.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema235.properties.resource.properties, key1))){
validate116.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.created_at !== undefined){
let data2 = data1.created_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate116.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate116.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.credential_epoch !== undefined){
let data3 = data1.credential_epoch;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(typeof data3 === "string"){
if(func2(data3) > 19){
validate116.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data3) < 1){
validate116.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data3)){
validate116.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate116.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs9 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.downstream_id !== undefined){
let data4 = data1.downstream_id;
const _errs12 = errors;
const _errs13 = errors;
let valid3 = false;
let passing0 = null;
const _errs14 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/properties/resource/properties/downstream_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs14 === errors;
if(_valid0){
valid3 = true;
passing0 = 0;
}
const _errs16 = errors;
const _errs17 = errors;
if(errors === _errs17){
if(errors === _errs17){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err1 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err2 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs16 === errors;
if(_valid0 && valid3){
valid3 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid3 = true;
passing0 = 1;
}
}
if(!valid3){
const err4 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/properties/resource/properties/downstream_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate116.errors = vErrors;
return false;
}
else {
errors = _errs13;
if(vErrors !== null){
if(_errs13){
vErrors.length = _errs13;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs12 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.enabled !== undefined){
const _errs19 = errors;
if(typeof data1.enabled !== "boolean"){
validate116.errors = [{instancePath:instancePath+"/resource/enabled",schemaPath:"#/properties/resource/properties/enabled/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid1 = _errs19 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data6 = data1.id;
const _errs21 = errors;
const _errs22 = errors;
if(errors === _errs22){
if(errors === _errs22){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate116.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate116.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate116.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs21 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.kind !== undefined){
let data7 = data1.kind;
const _errs24 = errors;
if(typeof data7 !== "string"){
validate116.errors = [{instancePath:instancePath+"/resource/kind",schemaPath:"#/components/schemas/PrincipalKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data7 === "CLI") || (data7 === "DOWNSTREAM")) || (data7 === "AUTOMATION")) || (data7 === "MISSION"))){
validate116.errors = [{instancePath:instancePath+"/resource/kind",schemaPath:"#/components/schemas/PrincipalKind/enum",keyword:"enum",params:{allowedValues: schema79.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs24 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.name !== undefined){
const _errs27 = errors;
if(typeof data1.name !== "string"){
validate116.errors = [{instancePath:instancePath+"/resource/name",schemaPath:"#/properties/resource/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs27 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.project_id !== undefined){
let data9 = data1.project_id;
const _errs29 = errors;
const _errs30 = errors;
let valid7 = false;
let passing1 = null;
const _errs31 = errors;
if(data9 !== null){
const err5 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/properties/resource/properties/project_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs31 === errors;
if(_valid1){
valid7 = true;
passing1 = 0;
}
const _errs33 = errors;
const _errs34 = errors;
if(errors === _errs34){
if(errors === _errs34){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
const err6 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data9))){
const err7 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs33 === errors;
if(_valid1 && valid7){
valid7 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid7 = true;
passing1 = 1;
}
}
if(!valid7){
const err9 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/properties/resource/properties/project_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate116.errors = vErrors;
return false;
}
else {
errors = _errs30;
if(vErrors !== null){
if(_errs30){
vErrors.length = _errs30;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs29 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revision !== undefined){
let data10 = data1.revision;
const _errs36 = errors;
const _errs37 = errors;
if(errors === _errs37){
if(typeof data10 === "string"){
if(func2(data10) > 19){
validate116.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate116.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data10)){
validate116.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate116.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs36 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.run_id !== undefined){
let data11 = data1.run_id;
const _errs39 = errors;
const _errs40 = errors;
let valid10 = false;
let passing2 = null;
const _errs41 = errors;
if(data11 !== null){
const err10 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/properties/resource/properties/run_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
var _valid2 = _errs41 === errors;
if(_valid2){
valid10 = true;
passing2 = 0;
}
const _errs43 = errors;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err11 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err12 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
else {
const err13 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
}
var _valid2 = _errs43 === errors;
if(_valid2 && valid10){
valid10 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid10 = true;
passing2 = 1;
}
}
if(!valid10){
const err14 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/properties/resource/properties/run_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
validate116.errors = vErrors;
return false;
}
else {
errors = _errs40;
if(vErrors !== null){
if(_errs40){
vErrors.length = _errs40;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs39 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.updated_at !== undefined){
let data12 = data1.updated_at;
const _errs46 = errors;
if(errors === _errs46){
if(errors === _errs46){
if(typeof data12 === "string"){
if(!(formats0.validate(data12))){
validate116.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate116.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs46 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate116.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data13 = data.schema_version;
const _errs48 = errors;
const _errs49 = errors;
if(!((typeof data13 == "number") && (!(data13 % 1) && !isNaN(data13)))){
validate116.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data13 === 1)){
validate116.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs49){
if(typeof data13 == "number"){
if(data13 > 1 || isNaN(data13)){
validate116.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data13 < 1 || isNaN(data13)){
validate116.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs48 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate116.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate116.errors = vErrors;
return errors === 0;
}
validate116.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate115(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate115.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate116(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate116.errors : vErrors.concat(validate116.errors);
errors = vErrors.length;
}
validate115.errors = vErrors;
return errors === 0;
}
validate115.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response23 = validate118;
const schema244 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1machine-principals~1{id}/patch/responses/200/content/application~1json/schema"};

function validate119(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate119.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate119.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate119.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate119.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if(((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.name === undefined) && (missing1 = "name"))) || ((data1.kind === undefined) && (missing1 = "kind"))) || ((data1.enabled === undefined) && (missing1 = "enabled"))) || ((data1.credential_epoch === undefined) && (missing1 = "credential_epoch"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate119.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema235.properties.resource.properties, key1))){
validate119.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.created_at !== undefined){
let data2 = data1.created_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate119.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate119.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.credential_epoch !== undefined){
let data3 = data1.credential_epoch;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(typeof data3 === "string"){
if(func2(data3) > 19){
validate119.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data3) < 1){
validate119.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data3)){
validate119.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate119.errors = [{instancePath:instancePath+"/resource/credential_epoch",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs9 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.downstream_id !== undefined){
let data4 = data1.downstream_id;
const _errs12 = errors;
const _errs13 = errors;
let valid3 = false;
let passing0 = null;
const _errs14 = errors;
if(data4 !== null){
const err0 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/properties/resource/properties/downstream_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs14 === errors;
if(_valid0){
valid3 = true;
passing0 = 0;
}
const _errs16 = errors;
const _errs17 = errors;
if(errors === _errs17){
if(errors === _errs17){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err1 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err2 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs16 === errors;
if(_valid0 && valid3){
valid3 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid3 = true;
passing0 = 1;
}
}
if(!valid3){
const err4 = {instancePath:instancePath+"/resource/downstream_id",schemaPath:"#/properties/resource/properties/downstream_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate119.errors = vErrors;
return false;
}
else {
errors = _errs13;
if(vErrors !== null){
if(_errs13){
vErrors.length = _errs13;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs12 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.enabled !== undefined){
const _errs19 = errors;
if(typeof data1.enabled !== "boolean"){
validate119.errors = [{instancePath:instancePath+"/resource/enabled",schemaPath:"#/properties/resource/properties/enabled/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid1 = _errs19 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data6 = data1.id;
const _errs21 = errors;
const _errs22 = errors;
if(errors === _errs22){
if(errors === _errs22){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate119.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate119.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate119.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs21 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.kind !== undefined){
let data7 = data1.kind;
const _errs24 = errors;
if(typeof data7 !== "string"){
validate119.errors = [{instancePath:instancePath+"/resource/kind",schemaPath:"#/components/schemas/PrincipalKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data7 === "CLI") || (data7 === "DOWNSTREAM")) || (data7 === "AUTOMATION")) || (data7 === "MISSION"))){
validate119.errors = [{instancePath:instancePath+"/resource/kind",schemaPath:"#/components/schemas/PrincipalKind/enum",keyword:"enum",params:{allowedValues: schema79.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs24 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.name !== undefined){
const _errs27 = errors;
if(typeof data1.name !== "string"){
validate119.errors = [{instancePath:instancePath+"/resource/name",schemaPath:"#/properties/resource/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs27 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.project_id !== undefined){
let data9 = data1.project_id;
const _errs29 = errors;
const _errs30 = errors;
let valid7 = false;
let passing1 = null;
const _errs31 = errors;
if(data9 !== null){
const err5 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/properties/resource/properties/project_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs31 === errors;
if(_valid1){
valid7 = true;
passing1 = 0;
}
const _errs33 = errors;
const _errs34 = errors;
if(errors === _errs34){
if(errors === _errs34){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
const err6 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data9))){
const err7 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs33 === errors;
if(_valid1 && valid7){
valid7 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid7 = true;
passing1 = 1;
}
}
if(!valid7){
const err9 = {instancePath:instancePath+"/resource/project_id",schemaPath:"#/properties/resource/properties/project_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate119.errors = vErrors;
return false;
}
else {
errors = _errs30;
if(vErrors !== null){
if(_errs30){
vErrors.length = _errs30;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs29 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revision !== undefined){
let data10 = data1.revision;
const _errs36 = errors;
const _errs37 = errors;
if(errors === _errs37){
if(typeof data10 === "string"){
if(func2(data10) > 19){
validate119.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate119.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data10)){
validate119.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate119.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs36 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.run_id !== undefined){
let data11 = data1.run_id;
const _errs39 = errors;
const _errs40 = errors;
let valid10 = false;
let passing2 = null;
const _errs41 = errors;
if(data11 !== null){
const err10 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/properties/resource/properties/run_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
var _valid2 = _errs41 === errors;
if(_valid2){
valid10 = true;
passing2 = 0;
}
const _errs43 = errors;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err11 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err12 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
else {
const err13 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
}
var _valid2 = _errs43 === errors;
if(_valid2 && valid10){
valid10 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid10 = true;
passing2 = 1;
}
}
if(!valid10){
const err14 = {instancePath:instancePath+"/resource/run_id",schemaPath:"#/properties/resource/properties/run_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
validate119.errors = vErrors;
return false;
}
else {
errors = _errs40;
if(vErrors !== null){
if(_errs40){
vErrors.length = _errs40;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs39 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.updated_at !== undefined){
let data12 = data1.updated_at;
const _errs46 = errors;
if(errors === _errs46){
if(errors === _errs46){
if(typeof data12 === "string"){
if(!(formats0.validate(data12))){
validate119.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate119.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs46 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate119.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data13 = data.schema_version;
const _errs48 = errors;
const _errs49 = errors;
if(!((typeof data13 == "number") && (!(data13 % 1) && !isNaN(data13)))){
validate119.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data13 === 1)){
validate119.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs49){
if(typeof data13 == "number"){
if(data13 > 1 || isNaN(data13)){
validate119.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data13 < 1 || isNaN(data13)){
validate119.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs48 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate119.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate119.errors = vErrors;
return errors === 0;
}
validate119.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate118(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate118.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate119(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate119.errors : vErrors.concat(validate119.errors);
errors = vErrors.length;
}
validate118.errors = vErrors;
return errors === 0;
}
validate118.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response24 = validate121;
const schema254 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1machine-principals~1{id}~1credentials/get/responses/200/content/application~1json/schema"};
const schema255 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"expires_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"issued_at":{"format":"date-time","type":"string"},"principal_epoch":{"$ref":"#/components/schemas/Revision"},"principal_id":{"$ref":"#/components/schemas/Id"},"public_token_id":{"$ref":"#/components/schemas/Id"},"revoked_at":{"format":"date-time","type":["string","null"]},"scope_codes":{"items":{"$ref":"#/components/schemas/MachineScope"},"type":"array"}},"required":["id","principal_id","public_token_id","principal_epoch","scope_codes","issued_at","expires_at"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};

function validate122(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate122.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate122.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate122.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.principal_id === undefined) && (missing1 = "principal_id"))) || ((data1.public_token_id === undefined) && (missing1 = "public_token_id"))) || ((data1.principal_epoch === undefined) && (missing1 = "principal_epoch"))) || ((data1.scope_codes === undefined) && (missing1 = "scope_codes"))) || ((data1.issued_at === undefined) && (missing1 = "issued_at"))) || ((data1.expires_at === undefined) && (missing1 = "expires_at"))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!((((((((key1 === "expires_at") || (key1 === "id")) || (key1 === "issued_at")) || (key1 === "principal_epoch")) || (key1 === "principal_id")) || (key1 === "public_token_id")) || (key1 === "revoked_at")) || (key1 === "scope_codes"))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.expires_at !== undefined){
let data2 = data1.expires_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/expires_at",schemaPath:"#/properties/items/items/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/expires_at",schemaPath:"#/properties/items/items/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data3 = data1.id;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(errors === _errs10){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data3))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs9 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.issued_at !== undefined){
let data4 = data1.issued_at;
const _errs12 = errors;
if(errors === _errs12){
if(errors === _errs12){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/issued_at",schemaPath:"#/properties/items/items/properties/issued_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/issued_at",schemaPath:"#/properties/items/items/properties/issued_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs12 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.principal_epoch !== undefined){
let data5 = data1.principal_epoch;
const _errs14 = errors;
const _errs15 = errors;
if(errors === _errs15){
if(typeof data5 === "string"){
if(func2(data5) > 19){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/principal_epoch",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data5) < 1){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/principal_epoch",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data5)){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/principal_epoch",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/principal_epoch",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs14 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.principal_id !== undefined){
let data6 = data1.principal_id;
const _errs17 = errors;
const _errs18 = errors;
if(errors === _errs18){
if(errors === _errs18){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/principal_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/principal_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/principal_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs17 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.public_token_id !== undefined){
let data7 = data1.public_token_id;
const _errs20 = errors;
const _errs21 = errors;
if(errors === _errs21){
if(errors === _errs21){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/public_token_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/public_token_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/public_token_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs20 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.revoked_at !== undefined){
let data8 = data1.revoked_at;
const _errs23 = errors;
if((typeof data8 !== "string") && (data8 !== null)){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/revoked_at",schemaPath:"#/properties/items/items/properties/revoked_at/type",keyword:"type",params:{type: schema255.properties.items.items.properties.revoked_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs23){
if(errors === _errs23){
if(typeof data8 === "string"){
if(!(formats0.validate(data8))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/revoked_at",schemaPath:"#/properties/items/items/properties/revoked_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid2 = _errs23 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.scope_codes !== undefined){
let data9 = data1.scope_codes;
const _errs25 = errors;
if(errors === _errs25){
if(Array.isArray(data9)){
var valid7 = true;
const len1 = data9.length;
for(let i1=0; i1<len1; i1++){
let data10 = data9[i1];
const _errs27 = errors;
if(typeof data10 !== "string"){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/scope_codes/" + i1,schemaPath:"#/components/schemas/MachineScope/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((((data10 === "RESEARCH_READ") || (data10 === "EXPERIMENT_SUBMIT")) || (data10 === "ARTIFACT_SUBMIT")) || (data10 === "EVIDENCE_READ")) || (data10 === "RUN_READ")) || (data10 === "RUN_CANCEL")) || (data10 === "DOWNSTREAM_CLAIM")) || (data10 === "DOWNSTREAM_ACK")) || (data10 === "FORWARD_SUBMIT")) || (data10 === "DOCTOR_READ"))){
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/scope_codes/" + i1,schemaPath:"#/components/schemas/MachineScope/enum",keyword:"enum",params:{allowedValues: schema83.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid7 = _errs27 === errors;
if(!valid7){
break;
}
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0+"/scope_codes",schemaPath:"#/properties/items/items/properties/scope_codes/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid2 = _errs25 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate122.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data11 = data.next_cursor;
const _errs30 = errors;
const _errs31 = errors;
let valid9 = false;
let passing0 = null;
const _errs32 = errors;
if(data11 !== null){
const err0 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs32 === errors;
if(_valid0){
valid9 = true;
passing0 = 0;
}
const _errs34 = errors;
const _errs35 = errors;
if(errors === _errs35){
if(errors === _errs35){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err1 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err2 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs34 === errors;
if(_valid0 && valid9){
valid9 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid9 = true;
passing0 = 1;
}
}
if(!valid9){
const err4 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate122.errors = vErrors;
return false;
}
else {
errors = _errs31;
if(vErrors !== null){
if(_errs31){
vErrors.length = _errs31;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs30 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data12 = data.schema_version;
const _errs37 = errors;
const _errs38 = errors;
if(!((typeof data12 == "number") && (!(data12 % 1) && !isNaN(data12)))){
validate122.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data12 === 1)){
validate122.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs38){
if(typeof data12 == "number"){
if(data12 > 1 || isNaN(data12)){
validate122.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data12 < 1 || isNaN(data12)){
validate122.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs37 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate122.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate122.errors = vErrors;
return errors === 0;
}
validate122.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate121(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate121.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate122(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate122.errors : vErrors.concat(validate122.errors);
errors = vErrors.length;
}
validate121.errors = vErrors;
return errors === 0;
}
validate121.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response25 = validate124;
const schema263 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1machine-principals~1{id}~1credentials/post/responses/201/content/application~1json/schema"};
const schema264 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"$ref":"#/components/schemas/CredentialView"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"token":{"description":"Only the initial response contains the token; it is never recoverable.","type":["string","null"]}},"required":["schema_version","replayed","resource"],"type":"object"};
const schema265 = {"additionalProperties":false,"properties":{"expires_at":{"format":"date-time","type":"string"},"id":{"$ref":"#/components/schemas/Id"},"issued_at":{"format":"date-time","type":"string"},"principal_epoch":{"$ref":"#/components/schemas/Revision"},"principal_id":{"$ref":"#/components/schemas/Id"},"public_token_id":{"$ref":"#/components/schemas/Id"},"revoked_at":{"format":"date-time","type":["string","null"]},"scope_codes":{"items":{"$ref":"#/components/schemas/MachineScope"},"type":"array"}},"required":["id","principal_id","public_token_id","principal_epoch","scope_codes","issued_at","expires_at"],"type":"object"};

function validate126(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate126.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((data.id === undefined) && (missing0 = "id")) || ((data.principal_id === undefined) && (missing0 = "principal_id"))) || ((data.public_token_id === undefined) && (missing0 = "public_token_id"))) || ((data.principal_epoch === undefined) && (missing0 = "principal_epoch"))) || ((data.scope_codes === undefined) && (missing0 = "scope_codes"))) || ((data.issued_at === undefined) && (missing0 = "issued_at"))) || ((data.expires_at === undefined) && (missing0 = "expires_at"))){
validate126.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((((((key0 === "expires_at") || (key0 === "id")) || (key0 === "issued_at")) || (key0 === "principal_epoch")) || (key0 === "principal_id")) || (key0 === "public_token_id")) || (key0 === "revoked_at")) || (key0 === "scope_codes"))){
validate126.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.expires_at !== undefined){
let data0 = data.expires_at;
const _errs2 = errors;
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate126.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate126.errors = [{instancePath:instancePath+"/expires_at",schemaPath:"#/properties/expires_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data1 = data.id;
const _errs4 = errors;
const _errs5 = errors;
if(errors === _errs5){
if(errors === _errs5){
if(typeof data1 === "string"){
if(!pattern5.test(data1)){
validate126.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data1))){
validate126.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate126.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.issued_at !== undefined){
let data2 = data.issued_at;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate126.errors = [{instancePath:instancePath+"/issued_at",schemaPath:"#/properties/issued_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate126.errors = [{instancePath:instancePath+"/issued_at",schemaPath:"#/properties/issued_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.principal_epoch !== undefined){
let data3 = data.principal_epoch;
const _errs9 = errors;
const _errs10 = errors;
if(errors === _errs10){
if(typeof data3 === "string"){
if(func2(data3) > 19){
validate126.errors = [{instancePath:instancePath+"/principal_epoch",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data3) < 1){
validate126.errors = [{instancePath:instancePath+"/principal_epoch",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data3)){
validate126.errors = [{instancePath:instancePath+"/principal_epoch",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate126.errors = [{instancePath:instancePath+"/principal_epoch",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.principal_id !== undefined){
let data4 = data.principal_id;
const _errs12 = errors;
const _errs13 = errors;
if(errors === _errs13){
if(errors === _errs13){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
validate126.errors = [{instancePath:instancePath+"/principal_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data4))){
validate126.errors = [{instancePath:instancePath+"/principal_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate126.errors = [{instancePath:instancePath+"/principal_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs12 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.public_token_id !== undefined){
let data5 = data.public_token_id;
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
validate126.errors = [{instancePath:instancePath+"/public_token_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data5))){
validate126.errors = [{instancePath:instancePath+"/public_token_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate126.errors = [{instancePath:instancePath+"/public_token_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs15 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revoked_at !== undefined){
let data6 = data.revoked_at;
const _errs18 = errors;
if((typeof data6 !== "string") && (data6 !== null)){
validate126.errors = [{instancePath:instancePath+"/revoked_at",schemaPath:"#/properties/revoked_at/type",keyword:"type",params:{type: schema265.properties.revoked_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs18){
if(errors === _errs18){
if(typeof data6 === "string"){
if(!(formats0.validate(data6))){
validate126.errors = [{instancePath:instancePath+"/revoked_at",schemaPath:"#/properties/revoked_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs18 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.scope_codes !== undefined){
let data7 = data.scope_codes;
const _errs20 = errors;
if(errors === _errs20){
if(Array.isArray(data7)){
var valid5 = true;
const len0 = data7.length;
for(let i0=0; i0<len0; i0++){
let data8 = data7[i0];
const _errs22 = errors;
if(typeof data8 !== "string"){
validate126.errors = [{instancePath:instancePath+"/scope_codes/" + i0,schemaPath:"#/components/schemas/MachineScope/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((((data8 === "RESEARCH_READ") || (data8 === "EXPERIMENT_SUBMIT")) || (data8 === "ARTIFACT_SUBMIT")) || (data8 === "EVIDENCE_READ")) || (data8 === "RUN_READ")) || (data8 === "RUN_CANCEL")) || (data8 === "DOWNSTREAM_CLAIM")) || (data8 === "DOWNSTREAM_ACK")) || (data8 === "FORWARD_SUBMIT")) || (data8 === "DOCTOR_READ"))){
validate126.errors = [{instancePath:instancePath+"/scope_codes/" + i0,schemaPath:"#/components/schemas/MachineScope/enum",keyword:"enum",params:{allowedValues: schema83.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid5 = _errs22 === errors;
if(!valid5){
break;
}
}
}
else {
validate126.errors = [{instancePath:instancePath+"/scope_codes",schemaPath:"#/properties/scope_codes/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs20 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
else {
validate126.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate126.errors = vErrors;
return errors === 0;
}
validate126.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate125(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate125.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate125.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version")) || (key0 === "token"))){
validate125.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate125.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
const _errs4 = errors;
if(!(validate126(data.resource, {instancePath:instancePath+"/resource",parentData:data,parentDataProperty:"resource",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate126.errors : vErrors.concat(validate126.errors);
errors = vErrors.length;
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data2 = data.schema_version;
const _errs5 = errors;
const _errs6 = errors;
if(!((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2)))){
validate125.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data2 === 1)){
validate125.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs6){
if(typeof data2 == "number"){
if(data2 > 1 || isNaN(data2)){
validate125.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data2 < 1 || isNaN(data2)){
validate125.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.token !== undefined){
let data3 = data.token;
const _errs8 = errors;
if((typeof data3 !== "string") && (data3 !== null)){
validate125.errors = [{instancePath:instancePath+"/token",schemaPath:"#/properties/token/type",keyword:"type",params:{type: schema264.properties.token.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate125.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate125.errors = vErrors;
return errors === 0;
}
validate125.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate124(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate124.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate125(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate125.errors : vErrors.concat(validate125.errors);
errors = vErrors.length;
}
validate124.errors = vErrors;
return errors === 0;
}
validate124.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response26 = validate129;
const schema272 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1projects/get/responses/200/content/application~1json/schema"};
const schema273 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"archived_at":{"format":"date-time","type":["string","null"]},"created_at":{"format":"date-time","type":"string"},"created_by":{"$ref":"#/components/schemas/ProjectOrigin"},"current_automation_policy_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"current_brief_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"description":{"type":"string"},"id":{"$ref":"#/components/schemas/Id"},"name":{"type":"string"},"revision":{"$ref":"#/components/schemas/Revision"},"root_lineage_id":{"$ref":"#/components/schemas/Id"},"state":{"$ref":"#/components/schemas/ProjectState"},"updated_at":{"format":"date-time","type":"string"}},"required":["id","root_lineage_id","name","description","state","created_by","created_at","updated_at","revision"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};
const schema274 = {"enum":["OPERATOR","IMPORT"],"type":"string"};
const schema280 = {"enum":["DRAFT","ACTIVE","PAUSED","ARCHIVED"],"type":"string"};

function validate130(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate130.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate130.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate130.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.root_lineage_id === undefined) && (missing1 = "root_lineage_id"))) || ((data1.name === undefined) && (missing1 = "name"))) || ((data1.description === undefined) && (missing1 = "description"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.created_by === undefined) && (missing1 = "created_by"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema273.properties.items.items.properties, key1))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.archived_at !== undefined){
let data2 = data1.archived_at;
const _errs7 = errors;
if((typeof data2 !== "string") && (data2 !== null)){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/archived_at",schemaPath:"#/properties/items/items/properties/archived_at/type",keyword:"type",params:{type: schema273.properties.items.items.properties.archived_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/archived_at",schemaPath:"#/properties/items/items/properties/archived_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.created_at !== undefined){
let data3 = data1.created_at;
const _errs9 = errors;
if(errors === _errs9){
if(errors === _errs9){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs9 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.created_by !== undefined){
let data4 = data1.created_by;
const _errs11 = errors;
if(typeof data4 !== "string"){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/created_by",schemaPath:"#/components/schemas/ProjectOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data4 === "OPERATOR") || (data4 === "IMPORT"))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/created_by",schemaPath:"#/components/schemas/ProjectOrigin/enum",keyword:"enum",params:{allowedValues: schema274.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs11 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.current_automation_policy_id !== undefined){
let data5 = data1.current_automation_policy_id;
const _errs14 = errors;
const _errs15 = errors;
let valid4 = false;
let passing0 = null;
const _errs16 = errors;
if(data5 !== null){
const err0 = {instancePath:instancePath+"/items/" + i0+"/current_automation_policy_id",schemaPath:"#/properties/items/items/properties/current_automation_policy_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs16 === errors;
if(_valid0){
valid4 = true;
passing0 = 0;
}
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
const err1 = {instancePath:instancePath+"/items/" + i0+"/current_automation_policy_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data5))){
const err2 = {instancePath:instancePath+"/items/" + i0+"/current_automation_policy_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/items/" + i0+"/current_automation_policy_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs18 === errors;
if(_valid0 && valid4){
valid4 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid4 = true;
passing0 = 1;
}
}
if(!valid4){
const err4 = {instancePath:instancePath+"/items/" + i0+"/current_automation_policy_id",schemaPath:"#/properties/items/items/properties/current_automation_policy_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate130.errors = vErrors;
return false;
}
else {
errors = _errs15;
if(vErrors !== null){
if(_errs15){
vErrors.length = _errs15;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs14 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.current_brief_id !== undefined){
let data6 = data1.current_brief_id;
const _errs21 = errors;
const _errs22 = errors;
let valid6 = false;
let passing1 = null;
const _errs23 = errors;
if(data6 !== null){
const err5 = {instancePath:instancePath+"/items/" + i0+"/current_brief_id",schemaPath:"#/properties/items/items/properties/current_brief_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs23 === errors;
if(_valid1){
valid6 = true;
passing1 = 0;
}
const _errs25 = errors;
const _errs26 = errors;
if(errors === _errs26){
if(errors === _errs26){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
const err6 = {instancePath:instancePath+"/items/" + i0+"/current_brief_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data6))){
const err7 = {instancePath:instancePath+"/items/" + i0+"/current_brief_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/items/" + i0+"/current_brief_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs25 === errors;
if(_valid1 && valid6){
valid6 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid6 = true;
passing1 = 1;
}
}
if(!valid6){
const err9 = {instancePath:instancePath+"/items/" + i0+"/current_brief_id",schemaPath:"#/properties/items/items/properties/current_brief_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate130.errors = vErrors;
return false;
}
else {
errors = _errs22;
if(vErrors !== null){
if(_errs22){
vErrors.length = _errs22;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs21 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.description !== undefined){
const _errs28 = errors;
if(typeof data1.description !== "string"){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/description",schemaPath:"#/properties/items/items/properties/description/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs28 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data8 = data1.id;
const _errs30 = errors;
const _errs31 = errors;
if(errors === _errs31){
if(errors === _errs31){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs30 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.name !== undefined){
const _errs33 = errors;
if(typeof data1.name !== "string"){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/name",schemaPath:"#/properties/items/items/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs33 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.revision !== undefined){
let data10 = data1.revision;
const _errs35 = errors;
const _errs36 = errors;
if(errors === _errs36){
if(typeof data10 === "string"){
if(func2(data10) > 19){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data10)){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs35 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.root_lineage_id !== undefined){
let data11 = data1.root_lineage_id;
const _errs38 = errors;
const _errs39 = errors;
if(errors === _errs39){
if(errors === _errs39){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/root_lineage_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data11))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/root_lineage_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/root_lineage_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs38 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.state !== undefined){
let data12 = data1.state;
const _errs41 = errors;
if(typeof data12 !== "string"){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/state",schemaPath:"#/components/schemas/ProjectState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data12 === "DRAFT") || (data12 === "ACTIVE")) || (data12 === "PAUSED")) || (data12 === "ARCHIVED"))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/state",schemaPath:"#/components/schemas/ProjectState/enum",keyword:"enum",params:{allowedValues: schema280.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs41 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.updated_at !== undefined){
let data13 = data1.updated_at;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data13 === "string"){
if(!(formats0.validate(data13))){
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/updated_at",schemaPath:"#/properties/items/items/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate130.errors = [{instancePath:instancePath+"/items/" + i0+"/updated_at",schemaPath:"#/properties/items/items/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs44 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate130.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate130.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data14 = data.next_cursor;
const _errs46 = errors;
const _errs47 = errors;
let valid12 = false;
let passing2 = null;
const _errs48 = errors;
if(data14 !== null){
const err10 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
var _valid2 = _errs48 === errors;
if(_valid2){
valid12 = true;
passing2 = 0;
}
const _errs50 = errors;
const _errs51 = errors;
if(errors === _errs51){
if(errors === _errs51){
if(typeof data14 === "string"){
if(!pattern5.test(data14)){
const err11 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
if(!(formats2.test(data14))){
const err12 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
else {
const err13 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
}
var _valid2 = _errs50 === errors;
if(_valid2 && valid12){
valid12 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid12 = true;
passing2 = 1;
}
}
if(!valid12){
const err14 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
validate130.errors = vErrors;
return false;
}
else {
errors = _errs47;
if(vErrors !== null){
if(_errs47){
vErrors.length = _errs47;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs46 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data15 = data.schema_version;
const _errs53 = errors;
const _errs54 = errors;
if(!((typeof data15 == "number") && (!(data15 % 1) && !isNaN(data15)))){
validate130.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data15 === 1)){
validate130.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs54){
if(typeof data15 == "number"){
if(data15 > 1 || isNaN(data15)){
validate130.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data15 < 1 || isNaN(data15)){
validate130.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs53 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate130.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate130.errors = vErrors;
return errors === 0;
}
validate130.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate129(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate129.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate130(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate130.errors : vErrors.concat(validate130.errors);
errors = vErrors.length;
}
validate129.errors = vErrors;
return errors === 0;
}
validate129.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response27 = validate132;
const schema283 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1projects/post/responses/201/content/application~1json/schema"};
const schema284 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"archived_at":{"format":"date-time","type":["string","null"]},"created_at":{"format":"date-time","type":"string"},"created_by":{"$ref":"#/components/schemas/ProjectOrigin"},"current_automation_policy_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"current_brief_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"description":{"type":"string"},"id":{"$ref":"#/components/schemas/Id"},"name":{"type":"string"},"revision":{"$ref":"#/components/schemas/Revision"},"root_lineage_id":{"$ref":"#/components/schemas/Id"},"state":{"$ref":"#/components/schemas/ProjectState"},"updated_at":{"format":"date-time","type":"string"}},"required":["id","root_lineage_id","name","description","state","created_by","created_at","updated_at","revision"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};

function validate133(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate133.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate133.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate133.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate133.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.root_lineage_id === undefined) && (missing1 = "root_lineage_id"))) || ((data1.name === undefined) && (missing1 = "name"))) || ((data1.description === undefined) && (missing1 = "description"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.created_by === undefined) && (missing1 = "created_by"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate133.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema284.properties.resource.properties, key1))){
validate133.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.archived_at !== undefined){
let data2 = data1.archived_at;
const _errs7 = errors;
if((typeof data2 !== "string") && (data2 !== null)){
validate133.errors = [{instancePath:instancePath+"/resource/archived_at",schemaPath:"#/properties/resource/properties/archived_at/type",keyword:"type",params:{type: schema284.properties.resource.properties.archived_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate133.errors = [{instancePath:instancePath+"/resource/archived_at",schemaPath:"#/properties/resource/properties/archived_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_at !== undefined){
let data3 = data1.created_at;
const _errs9 = errors;
if(errors === _errs9){
if(errors === _errs9){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate133.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate133.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs9 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_by !== undefined){
let data4 = data1.created_by;
const _errs11 = errors;
if(typeof data4 !== "string"){
validate133.errors = [{instancePath:instancePath+"/resource/created_by",schemaPath:"#/components/schemas/ProjectOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data4 === "OPERATOR") || (data4 === "IMPORT"))){
validate133.errors = [{instancePath:instancePath+"/resource/created_by",schemaPath:"#/components/schemas/ProjectOrigin/enum",keyword:"enum",params:{allowedValues: schema274.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs11 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.current_automation_policy_id !== undefined){
let data5 = data1.current_automation_policy_id;
const _errs14 = errors;
const _errs15 = errors;
let valid3 = false;
let passing0 = null;
const _errs16 = errors;
if(data5 !== null){
const err0 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/properties/resource/properties/current_automation_policy_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs16 === errors;
if(_valid0){
valid3 = true;
passing0 = 0;
}
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
const err1 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data5))){
const err2 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs18 === errors;
if(_valid0 && valid3){
valid3 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid3 = true;
passing0 = 1;
}
}
if(!valid3){
const err4 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/properties/resource/properties/current_automation_policy_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate133.errors = vErrors;
return false;
}
else {
errors = _errs15;
if(vErrors !== null){
if(_errs15){
vErrors.length = _errs15;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs14 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.current_brief_id !== undefined){
let data6 = data1.current_brief_id;
const _errs21 = errors;
const _errs22 = errors;
let valid5 = false;
let passing1 = null;
const _errs23 = errors;
if(data6 !== null){
const err5 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/properties/resource/properties/current_brief_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs23 === errors;
if(_valid1){
valid5 = true;
passing1 = 0;
}
const _errs25 = errors;
const _errs26 = errors;
if(errors === _errs26){
if(errors === _errs26){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
const err6 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data6))){
const err7 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs25 === errors;
if(_valid1 && valid5){
valid5 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid5 = true;
passing1 = 1;
}
}
if(!valid5){
const err9 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/properties/resource/properties/current_brief_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate133.errors = vErrors;
return false;
}
else {
errors = _errs22;
if(vErrors !== null){
if(_errs22){
vErrors.length = _errs22;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs21 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.description !== undefined){
const _errs28 = errors;
if(typeof data1.description !== "string"){
validate133.errors = [{instancePath:instancePath+"/resource/description",schemaPath:"#/properties/resource/properties/description/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs28 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data8 = data1.id;
const _errs30 = errors;
const _errs31 = errors;
if(errors === _errs31){
if(errors === _errs31){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate133.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate133.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate133.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs30 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.name !== undefined){
const _errs33 = errors;
if(typeof data1.name !== "string"){
validate133.errors = [{instancePath:instancePath+"/resource/name",schemaPath:"#/properties/resource/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs33 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revision !== undefined){
let data10 = data1.revision;
const _errs35 = errors;
const _errs36 = errors;
if(errors === _errs36){
if(typeof data10 === "string"){
if(func2(data10) > 19){
validate133.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate133.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data10)){
validate133.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate133.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs35 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.root_lineage_id !== undefined){
let data11 = data1.root_lineage_id;
const _errs38 = errors;
const _errs39 = errors;
if(errors === _errs39){
if(errors === _errs39){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
validate133.errors = [{instancePath:instancePath+"/resource/root_lineage_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data11))){
validate133.errors = [{instancePath:instancePath+"/resource/root_lineage_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate133.errors = [{instancePath:instancePath+"/resource/root_lineage_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs38 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.state !== undefined){
let data12 = data1.state;
const _errs41 = errors;
if(typeof data12 !== "string"){
validate133.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/ProjectState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data12 === "DRAFT") || (data12 === "ACTIVE")) || (data12 === "PAUSED")) || (data12 === "ARCHIVED"))){
validate133.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/ProjectState/enum",keyword:"enum",params:{allowedValues: schema280.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs41 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.updated_at !== undefined){
let data13 = data1.updated_at;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data13 === "string"){
if(!(formats0.validate(data13))){
validate133.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate133.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs44 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate133.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data14 = data.schema_version;
const _errs46 = errors;
const _errs47 = errors;
if(!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))){
validate133.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data14 === 1)){
validate133.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs47){
if(typeof data14 == "number"){
if(data14 > 1 || isNaN(data14)){
validate133.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data14 < 1 || isNaN(data14)){
validate133.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs46 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate133.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate133.errors = vErrors;
return errors === 0;
}
validate133.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate132(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate132.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate133(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate133.errors : vErrors.concat(validate133.errors);
errors = vErrors.length;
}
validate132.errors = vErrors;
return errors === 0;
}
validate132.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response28 = validate135;
const schema293 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1projects~1{id}/get/responses/200/content/application~1json/schema"};
const schema294 = {"additionalProperties":false,"properties":{"archived_at":{"format":"date-time","type":["string","null"]},"created_at":{"format":"date-time","type":"string"},"created_by":{"$ref":"#/components/schemas/ProjectOrigin"},"current_automation_policy_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"current_brief_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"description":{"type":"string"},"id":{"$ref":"#/components/schemas/Id"},"name":{"type":"string"},"revision":{"$ref":"#/components/schemas/Revision"},"root_lineage_id":{"$ref":"#/components/schemas/Id"},"state":{"$ref":"#/components/schemas/ProjectState"},"updated_at":{"format":"date-time","type":"string"}},"required":["id","root_lineage_id","name","description","state","created_by","created_at","updated_at","revision"],"type":"object"};

function validate136(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate136.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((((data.id === undefined) && (missing0 = "id")) || ((data.root_lineage_id === undefined) && (missing0 = "root_lineage_id"))) || ((data.name === undefined) && (missing0 = "name"))) || ((data.description === undefined) && (missing0 = "description"))) || ((data.state === undefined) && (missing0 = "state"))) || ((data.created_by === undefined) && (missing0 = "created_by"))) || ((data.created_at === undefined) && (missing0 = "created_at"))) || ((data.updated_at === undefined) && (missing0 = "updated_at"))) || ((data.revision === undefined) && (missing0 = "revision"))){
validate136.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema294.properties, key0))){
validate136.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.archived_at !== undefined){
let data0 = data.archived_at;
const _errs2 = errors;
if((typeof data0 !== "string") && (data0 !== null)){
validate136.errors = [{instancePath:instancePath+"/archived_at",schemaPath:"#/properties/archived_at/type",keyword:"type",params:{type: schema294.properties.archived_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs2){
if(errors === _errs2){
if(typeof data0 === "string"){
if(!(formats0.validate(data0))){
validate136.errors = [{instancePath:instancePath+"/archived_at",schemaPath:"#/properties/archived_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_at !== undefined){
let data1 = data.created_at;
const _errs4 = errors;
if(errors === _errs4){
if(errors === _errs4){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate136.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate136.errors = [{instancePath:instancePath+"/created_at",schemaPath:"#/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_by !== undefined){
let data2 = data.created_by;
const _errs6 = errors;
if(typeof data2 !== "string"){
validate136.errors = [{instancePath:instancePath+"/created_by",schemaPath:"#/components/schemas/ProjectOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data2 === "OPERATOR") || (data2 === "IMPORT"))){
validate136.errors = [{instancePath:instancePath+"/created_by",schemaPath:"#/components/schemas/ProjectOrigin/enum",keyword:"enum",params:{allowedValues: schema274.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.current_automation_policy_id !== undefined){
let data3 = data.current_automation_policy_id;
const _errs9 = errors;
const _errs10 = errors;
let valid2 = false;
let passing0 = null;
const _errs11 = errors;
if(data3 !== null){
const err0 = {instancePath:instancePath+"/current_automation_policy_id",schemaPath:"#/properties/current_automation_policy_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs11 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs13 = errors;
const _errs14 = errors;
if(errors === _errs14){
if(errors === _errs14){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
const err1 = {instancePath:instancePath+"/current_automation_policy_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data3))){
const err2 = {instancePath:instancePath+"/current_automation_policy_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/current_automation_policy_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs13 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/current_automation_policy_id",schemaPath:"#/properties/current_automation_policy_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate136.errors = vErrors;
return false;
}
else {
errors = _errs10;
if(vErrors !== null){
if(_errs10){
vErrors.length = _errs10;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.current_brief_id !== undefined){
let data4 = data.current_brief_id;
const _errs16 = errors;
const _errs17 = errors;
let valid4 = false;
let passing1 = null;
const _errs18 = errors;
if(data4 !== null){
const err5 = {instancePath:instancePath+"/current_brief_id",schemaPath:"#/properties/current_brief_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs18 === errors;
if(_valid1){
valid4 = true;
passing1 = 0;
}
const _errs20 = errors;
const _errs21 = errors;
if(errors === _errs21){
if(errors === _errs21){
if(typeof data4 === "string"){
if(!pattern5.test(data4)){
const err6 = {instancePath:instancePath+"/current_brief_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data4))){
const err7 = {instancePath:instancePath+"/current_brief_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/current_brief_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs20 === errors;
if(_valid1 && valid4){
valid4 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid4 = true;
passing1 = 1;
}
}
if(!valid4){
const err9 = {instancePath:instancePath+"/current_brief_id",schemaPath:"#/properties/current_brief_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate136.errors = vErrors;
return false;
}
else {
errors = _errs17;
if(vErrors !== null){
if(_errs17){
vErrors.length = _errs17;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.description !== undefined){
const _errs23 = errors;
if(typeof data.description !== "string"){
validate136.errors = [{instancePath:instancePath+"/description",schemaPath:"#/properties/description/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs23 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data6 = data.id;
const _errs25 = errors;
const _errs26 = errors;
if(errors === _errs26){
if(errors === _errs26){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate136.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate136.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate136.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs25 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.name !== undefined){
const _errs28 = errors;
if(typeof data.name !== "string"){
validate136.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs28 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revision !== undefined){
let data8 = data.revision;
const _errs30 = errors;
const _errs31 = errors;
if(errors === _errs31){
if(typeof data8 === "string"){
if(func2(data8) > 19){
validate136.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data8) < 1){
validate136.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data8)){
validate136.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate136.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs30 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.root_lineage_id !== undefined){
let data9 = data.root_lineage_id;
const _errs33 = errors;
const _errs34 = errors;
if(errors === _errs34){
if(errors === _errs34){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
validate136.errors = [{instancePath:instancePath+"/root_lineage_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data9))){
validate136.errors = [{instancePath:instancePath+"/root_lineage_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate136.errors = [{instancePath:instancePath+"/root_lineage_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs33 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.state !== undefined){
let data10 = data.state;
const _errs36 = errors;
if(typeof data10 !== "string"){
validate136.errors = [{instancePath:instancePath+"/state",schemaPath:"#/components/schemas/ProjectState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data10 === "DRAFT") || (data10 === "ACTIVE")) || (data10 === "PAUSED")) || (data10 === "ARCHIVED"))){
validate136.errors = [{instancePath:instancePath+"/state",schemaPath:"#/components/schemas/ProjectState/enum",keyword:"enum",params:{allowedValues: schema280.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs36 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.updated_at !== undefined){
let data11 = data.updated_at;
const _errs39 = errors;
if(errors === _errs39){
if(errors === _errs39){
if(typeof data11 === "string"){
if(!(formats0.validate(data11))){
validate136.errors = [{instancePath:instancePath+"/updated_at",schemaPath:"#/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate136.errors = [{instancePath:instancePath+"/updated_at",schemaPath:"#/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs39 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate136.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate136.errors = vErrors;
return errors === 0;
}
validate136.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate135(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate135.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate136(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate136.errors : vErrors.concat(validate136.errors);
errors = vErrors.length;
}
validate135.errors = vErrors;
return errors === 0;
}
validate135.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response29 = validate138;
const schema302 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1projects~1{id}/patch/responses/200/content/application~1json/schema"};

function validate139(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate139.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate139.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate139.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate139.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.root_lineage_id === undefined) && (missing1 = "root_lineage_id"))) || ((data1.name === undefined) && (missing1 = "name"))) || ((data1.description === undefined) && (missing1 = "description"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.created_by === undefined) && (missing1 = "created_by"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate139.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema284.properties.resource.properties, key1))){
validate139.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.archived_at !== undefined){
let data2 = data1.archived_at;
const _errs7 = errors;
if((typeof data2 !== "string") && (data2 !== null)){
validate139.errors = [{instancePath:instancePath+"/resource/archived_at",schemaPath:"#/properties/resource/properties/archived_at/type",keyword:"type",params:{type: schema284.properties.resource.properties.archived_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs7){
if(errors === _errs7){
if(typeof data2 === "string"){
if(!(formats0.validate(data2))){
validate139.errors = [{instancePath:instancePath+"/resource/archived_at",schemaPath:"#/properties/resource/properties/archived_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_at !== undefined){
let data3 = data1.created_at;
const _errs9 = errors;
if(errors === _errs9){
if(errors === _errs9){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate139.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate139.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs9 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_by !== undefined){
let data4 = data1.created_by;
const _errs11 = errors;
if(typeof data4 !== "string"){
validate139.errors = [{instancePath:instancePath+"/resource/created_by",schemaPath:"#/components/schemas/ProjectOrigin/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data4 === "OPERATOR") || (data4 === "IMPORT"))){
validate139.errors = [{instancePath:instancePath+"/resource/created_by",schemaPath:"#/components/schemas/ProjectOrigin/enum",keyword:"enum",params:{allowedValues: schema274.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs11 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.current_automation_policy_id !== undefined){
let data5 = data1.current_automation_policy_id;
const _errs14 = errors;
const _errs15 = errors;
let valid3 = false;
let passing0 = null;
const _errs16 = errors;
if(data5 !== null){
const err0 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/properties/resource/properties/current_automation_policy_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs16 === errors;
if(_valid0){
valid3 = true;
passing0 = 0;
}
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
const err1 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data5))){
const err2 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs18 === errors;
if(_valid0 && valid3){
valid3 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid3 = true;
passing0 = 1;
}
}
if(!valid3){
const err4 = {instancePath:instancePath+"/resource/current_automation_policy_id",schemaPath:"#/properties/resource/properties/current_automation_policy_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate139.errors = vErrors;
return false;
}
else {
errors = _errs15;
if(vErrors !== null){
if(_errs15){
vErrors.length = _errs15;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs14 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.current_brief_id !== undefined){
let data6 = data1.current_brief_id;
const _errs21 = errors;
const _errs22 = errors;
let valid5 = false;
let passing1 = null;
const _errs23 = errors;
if(data6 !== null){
const err5 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/properties/resource/properties/current_brief_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs23 === errors;
if(_valid1){
valid5 = true;
passing1 = 0;
}
const _errs25 = errors;
const _errs26 = errors;
if(errors === _errs26){
if(errors === _errs26){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
const err6 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data6))){
const err7 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs25 === errors;
if(_valid1 && valid5){
valid5 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid5 = true;
passing1 = 1;
}
}
if(!valid5){
const err9 = {instancePath:instancePath+"/resource/current_brief_id",schemaPath:"#/properties/resource/properties/current_brief_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate139.errors = vErrors;
return false;
}
else {
errors = _errs22;
if(vErrors !== null){
if(_errs22){
vErrors.length = _errs22;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs21 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.description !== undefined){
const _errs28 = errors;
if(typeof data1.description !== "string"){
validate139.errors = [{instancePath:instancePath+"/resource/description",schemaPath:"#/properties/resource/properties/description/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs28 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data8 = data1.id;
const _errs30 = errors;
const _errs31 = errors;
if(errors === _errs31){
if(errors === _errs31){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate139.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate139.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate139.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs30 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.name !== undefined){
const _errs33 = errors;
if(typeof data1.name !== "string"){
validate139.errors = [{instancePath:instancePath+"/resource/name",schemaPath:"#/properties/resource/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs33 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revision !== undefined){
let data10 = data1.revision;
const _errs35 = errors;
const _errs36 = errors;
if(errors === _errs36){
if(typeof data10 === "string"){
if(func2(data10) > 19){
validate139.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data10) < 1){
validate139.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data10)){
validate139.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate139.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs35 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.root_lineage_id !== undefined){
let data11 = data1.root_lineage_id;
const _errs38 = errors;
const _errs39 = errors;
if(errors === _errs39){
if(errors === _errs39){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
validate139.errors = [{instancePath:instancePath+"/resource/root_lineage_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data11))){
validate139.errors = [{instancePath:instancePath+"/resource/root_lineage_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate139.errors = [{instancePath:instancePath+"/resource/root_lineage_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs38 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.state !== undefined){
let data12 = data1.state;
const _errs41 = errors;
if(typeof data12 !== "string"){
validate139.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/ProjectState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((data12 === "DRAFT") || (data12 === "ACTIVE")) || (data12 === "PAUSED")) || (data12 === "ARCHIVED"))){
validate139.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/ProjectState/enum",keyword:"enum",params:{allowedValues: schema280.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs41 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.updated_at !== undefined){
let data13 = data1.updated_at;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data13 === "string"){
if(!(formats0.validate(data13))){
validate139.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate139.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs44 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate139.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data14 = data.schema_version;
const _errs46 = errors;
const _errs47 = errors;
if(!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))){
validate139.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data14 === 1)){
validate139.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs47){
if(typeof data14 == "number"){
if(data14 > 1 || isNaN(data14)){
validate139.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data14 < 1 || isNaN(data14)){
validate139.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs46 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate139.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate139.errors = vErrors;
return errors === 0;
}
validate139.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate138(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate138.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate139(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate139.errors : vErrors.concat(validate139.errors);
errors = vErrors.length;
}
validate138.errors = vErrors;
return errors === 0;
}
validate138.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response30 = validate141;
const schema312 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1projects~1{id}~1briefs/get/responses/200/content/application~1json/schema"};
const schema313 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"bindings":{"items":{"$ref":"#/components/schemas/BriefBindingV1"},"maxItems":64,"minItems":1,"type":"array"},"content":{"$ref":"#/components/schemas/BriefContentV1"},"created_at":{"format":"date-time","type":"string"},"frozen_at":{"format":"date-time","type":["string","null"]},"id":{"$ref":"#/components/schemas/Id"},"project_id":{"$ref":"#/components/schemas/Id"},"revision":{"$ref":"#/components/schemas/Revision"},"state":{"$ref":"#/components/schemas/BriefState"},"supersedes_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"updated_at":{"format":"date-time","type":"string"},"version":{"format":"int32","maximum":2147483647,"minimum":1,"type":"integer"}},"required":["id","project_id","version","revision","state","content","bindings","created_at","updated_at"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};

function validate142(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate142.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate142.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate142.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.version === undefined) && (missing1 = "version"))) || ((data1.revision === undefined) && (missing1 = "revision"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.content === undefined) && (missing1 = "content"))) || ((data1.bindings === undefined) && (missing1 = "bindings"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema313.properties.items.items.properties, key1))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.bindings !== undefined){
let data2 = data1.bindings;
const _errs7 = errors;
if(errors === _errs7){
if(Array.isArray(data2)){
if(data2.length > 64){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/bindings",schemaPath:"#/properties/items/items/properties/bindings/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data2.length < 1){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/bindings",schemaPath:"#/properties/items/items/properties/bindings/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid3 = true;
const len1 = data2.length;
for(let i1=0; i1<len1; i1++){
const _errs9 = errors;
if(!(validate61(data2[i1], {instancePath:instancePath+"/items/" + i0+"/bindings/" + i1,parentData:data2,parentDataProperty:i1,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate61.errors : vErrors.concat(validate61.errors);
errors = vErrors.length;
}
var valid3 = _errs9 === errors;
if(!valid3){
break;
}
}
}
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/bindings",schemaPath:"#/properties/items/items/properties/bindings/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.content !== undefined){
const _errs10 = errors;
if(!(validate63(data1.content, {instancePath:instancePath+"/items/" + i0+"/content",parentData:data1,parentDataProperty:"content",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate63.errors : vErrors.concat(validate63.errors);
errors = vErrors.length;
}
var valid2 = _errs10 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.created_at !== undefined){
let data5 = data1.created_at;
const _errs11 = errors;
if(errors === _errs11){
if(errors === _errs11){
if(typeof data5 === "string"){
if(!(formats0.validate(data5))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/created_at",schemaPath:"#/properties/items/items/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs11 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.frozen_at !== undefined){
let data6 = data1.frozen_at;
const _errs13 = errors;
if((typeof data6 !== "string") && (data6 !== null)){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/frozen_at",schemaPath:"#/properties/items/items/properties/frozen_at/type",keyword:"type",params:{type: schema313.properties.items.items.properties.frozen_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs13){
if(errors === _errs13){
if(typeof data6 === "string"){
if(!(formats0.validate(data6))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/frozen_at",schemaPath:"#/properties/items/items/properties/frozen_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid2 = _errs13 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data7 = data1.id;
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs15 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.project_id !== undefined){
let data8 = data1.project_id;
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs18 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.revision !== undefined){
let data9 = data1.revision;
const _errs21 = errors;
const _errs22 = errors;
if(errors === _errs22){
if(typeof data9 === "string"){
if(func2(data9) > 19){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data9) < 1){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data9)){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs21 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.state !== undefined){
let data10 = data1.state;
const _errs24 = errors;
if(typeof data10 !== "string"){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/state",schemaPath:"#/components/schemas/BriefState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data10 === "DRAFT") || (data10 === "FROZEN"))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/state",schemaPath:"#/components/schemas/BriefState/enum",keyword:"enum",params:{allowedValues: schema133.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs24 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.supersedes_id !== undefined){
let data11 = data1.supersedes_id;
const _errs27 = errors;
const _errs28 = errors;
let valid8 = false;
let passing0 = null;
const _errs29 = errors;
if(data11 !== null){
const err0 = {instancePath:instancePath+"/items/" + i0+"/supersedes_id",schemaPath:"#/properties/items/items/properties/supersedes_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs29 === errors;
if(_valid0){
valid8 = true;
passing0 = 0;
}
const _errs31 = errors;
const _errs32 = errors;
if(errors === _errs32){
if(errors === _errs32){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err1 = {instancePath:instancePath+"/items/" + i0+"/supersedes_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err2 = {instancePath:instancePath+"/items/" + i0+"/supersedes_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/items/" + i0+"/supersedes_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs31 === errors;
if(_valid0 && valid8){
valid8 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid8 = true;
passing0 = 1;
}
}
if(!valid8){
const err4 = {instancePath:instancePath+"/items/" + i0+"/supersedes_id",schemaPath:"#/properties/items/items/properties/supersedes_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate142.errors = vErrors;
return false;
}
else {
errors = _errs28;
if(vErrors !== null){
if(_errs28){
vErrors.length = _errs28;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs27 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.updated_at !== undefined){
let data12 = data1.updated_at;
const _errs34 = errors;
if(errors === _errs34){
if(errors === _errs34){
if(typeof data12 === "string"){
if(!(formats0.validate(data12))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/updated_at",schemaPath:"#/properties/items/items/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/updated_at",schemaPath:"#/properties/items/items/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs34 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.version !== undefined){
let data13 = data1.version;
const _errs36 = errors;
if(!((typeof data13 == "number") && (!(data13 % 1) && !isNaN(data13)))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs36){
if(typeof data13 == "number"){
if(data13 > 2147483647 || isNaN(data13)){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data13 < 1 || isNaN(data13)){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data13))){
validate142.errors = [{instancePath:instancePath+"/items/" + i0+"/version",schemaPath:"#/properties/items/items/properties/version/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid2 = _errs36 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate142.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data14 = data.next_cursor;
const _errs38 = errors;
const _errs39 = errors;
let valid10 = false;
let passing1 = null;
const _errs40 = errors;
if(data14 !== null){
const err5 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs40 === errors;
if(_valid1){
valid10 = true;
passing1 = 0;
}
const _errs42 = errors;
const _errs43 = errors;
if(errors === _errs43){
if(errors === _errs43){
if(typeof data14 === "string"){
if(!pattern5.test(data14)){
const err6 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data14))){
const err7 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs42 === errors;
if(_valid1 && valid10){
valid10 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid10 = true;
passing1 = 1;
}
}
if(!valid10){
const err9 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate142.errors = vErrors;
return false;
}
else {
errors = _errs39;
if(vErrors !== null){
if(_errs39){
vErrors.length = _errs39;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs38 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data15 = data.schema_version;
const _errs45 = errors;
const _errs46 = errors;
if(!((typeof data15 == "number") && (!(data15 % 1) && !isNaN(data15)))){
validate142.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data15 === 1)){
validate142.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs46){
if(typeof data15 == "number"){
if(data15 > 1 || isNaN(data15)){
validate142.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data15 < 1 || isNaN(data15)){
validate142.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs45 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate142.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate142.errors = vErrors;
return errors === 0;
}
validate142.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate141(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate141.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate142(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate142.errors : vErrors.concat(validate142.errors);
errors = vErrors.length;
}
validate141.errors = vErrors;
return errors === 0;
}
validate141.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response31 = validate146;
const schema321 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1projects~1{id}~1briefs/post/responses/201/content/application~1json/schema"};

function validate147(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate147.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate147.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate147.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate147.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((data1.id === undefined) && (missing1 = "id")) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.version === undefined) && (missing1 = "version"))) || ((data1.revision === undefined) && (missing1 = "revision"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.content === undefined) && (missing1 = "content"))) || ((data1.bindings === undefined) && (missing1 = "bindings"))) || ((data1.created_at === undefined) && (missing1 = "created_at"))) || ((data1.updated_at === undefined) && (missing1 = "updated_at"))){
validate147.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema136.properties.resource.properties, key1))){
validate147.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.bindings !== undefined){
let data2 = data1.bindings;
const _errs7 = errors;
if(errors === _errs7){
if(Array.isArray(data2)){
if(data2.length > 64){
validate147.errors = [{instancePath:instancePath+"/resource/bindings",schemaPath:"#/properties/resource/properties/bindings/maxItems",keyword:"maxItems",params:{limit: 64},message:"must NOT have more than 64 items"}];
return false;
}
else {
if(data2.length < 1){
validate147.errors = [{instancePath:instancePath+"/resource/bindings",schemaPath:"#/properties/resource/properties/bindings/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid2 = true;
const len0 = data2.length;
for(let i0=0; i0<len0; i0++){
const _errs9 = errors;
if(!(validate61(data2[i0], {instancePath:instancePath+"/resource/bindings/" + i0,parentData:data2,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate61.errors : vErrors.concat(validate61.errors);
errors = vErrors.length;
}
var valid2 = _errs9 === errors;
if(!valid2){
break;
}
}
}
}
}
else {
validate147.errors = [{instancePath:instancePath+"/resource/bindings",schemaPath:"#/properties/resource/properties/bindings/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.content !== undefined){
const _errs10 = errors;
if(!(validate63(data1.content, {instancePath:instancePath+"/resource/content",parentData:data1,parentDataProperty:"content",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate63.errors : vErrors.concat(validate63.errors);
errors = vErrors.length;
}
var valid1 = _errs10 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.created_at !== undefined){
let data5 = data1.created_at;
const _errs11 = errors;
if(errors === _errs11){
if(errors === _errs11){
if(typeof data5 === "string"){
if(!(formats0.validate(data5))){
validate147.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate147.errors = [{instancePath:instancePath+"/resource/created_at",schemaPath:"#/properties/resource/properties/created_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs11 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.frozen_at !== undefined){
let data6 = data1.frozen_at;
const _errs13 = errors;
if((typeof data6 !== "string") && (data6 !== null)){
validate147.errors = [{instancePath:instancePath+"/resource/frozen_at",schemaPath:"#/properties/resource/properties/frozen_at/type",keyword:"type",params:{type: schema136.properties.resource.properties.frozen_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs13){
if(errors === _errs13){
if(typeof data6 === "string"){
if(!(formats0.validate(data6))){
validate147.errors = [{instancePath:instancePath+"/resource/frozen_at",schemaPath:"#/properties/resource/properties/frozen_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs13 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data7 = data1.id;
const _errs15 = errors;
const _errs16 = errors;
if(errors === _errs16){
if(errors === _errs16){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate147.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate147.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate147.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs15 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.project_id !== undefined){
let data8 = data1.project_id;
const _errs18 = errors;
const _errs19 = errors;
if(errors === _errs19){
if(errors === _errs19){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate147.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate147.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate147.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs18 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revision !== undefined){
let data9 = data1.revision;
const _errs21 = errors;
const _errs22 = errors;
if(errors === _errs22){
if(typeof data9 === "string"){
if(func2(data9) > 19){
validate147.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data9) < 1){
validate147.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data9)){
validate147.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate147.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs21 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.state !== undefined){
let data10 = data1.state;
const _errs24 = errors;
if(typeof data10 !== "string"){
validate147.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/BriefState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((data10 === "DRAFT") || (data10 === "FROZEN"))){
validate147.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/BriefState/enum",keyword:"enum",params:{allowedValues: schema133.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs24 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.supersedes_id !== undefined){
let data11 = data1.supersedes_id;
const _errs27 = errors;
const _errs28 = errors;
let valid7 = false;
let passing0 = null;
const _errs29 = errors;
if(data11 !== null){
const err0 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/properties/resource/properties/supersedes_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs29 === errors;
if(_valid0){
valid7 = true;
passing0 = 0;
}
const _errs31 = errors;
const _errs32 = errors;
if(errors === _errs32){
if(errors === _errs32){
if(typeof data11 === "string"){
if(!pattern5.test(data11)){
const err1 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data11))){
const err2 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs31 === errors;
if(_valid0 && valid7){
valid7 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid7 = true;
passing0 = 1;
}
}
if(!valid7){
const err4 = {instancePath:instancePath+"/resource/supersedes_id",schemaPath:"#/properties/resource/properties/supersedes_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate147.errors = vErrors;
return false;
}
else {
errors = _errs28;
if(vErrors !== null){
if(_errs28){
vErrors.length = _errs28;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs27 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.updated_at !== undefined){
let data12 = data1.updated_at;
const _errs34 = errors;
if(errors === _errs34){
if(errors === _errs34){
if(typeof data12 === "string"){
if(!(formats0.validate(data12))){
validate147.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate147.errors = [{instancePath:instancePath+"/resource/updated_at",schemaPath:"#/properties/resource/properties/updated_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs34 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.version !== undefined){
let data13 = data1.version;
const _errs36 = errors;
if(!((typeof data13 == "number") && (!(data13 % 1) && !isNaN(data13)))){
validate147.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs36){
if(typeof data13 == "number"){
if(data13 > 2147483647 || isNaN(data13)){
validate147.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 2147483647},message:"must be <= 2147483647"}];
return false;
}
else {
if(data13 < 1 || isNaN(data13)){
validate147.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
else {
if(!(formats94.validate(data13))){
validate147.errors = [{instancePath:instancePath+"/resource/version",schemaPath:"#/properties/resource/properties/version/format",keyword:"format",params:{format: "int32"},message:"must match format \""+"int32"+"\""}];
return false;
}
}
}
}
}
var valid1 = _errs36 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate147.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data14 = data.schema_version;
const _errs38 = errors;
const _errs39 = errors;
if(!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))){
validate147.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data14 === 1)){
validate147.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs39){
if(typeof data14 == "number"){
if(data14 > 1 || isNaN(data14)){
validate147.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data14 < 1 || isNaN(data14)){
validate147.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs38 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate147.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate147.errors = vErrors;
return errors === 0;
}
validate147.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate146(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate146.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate147(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate147.errors : vErrors.concat(validate147.errors);
errors = vErrors.length;
}
validate146.errors = vErrors;
return errors === 0;
}
validate146.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response32 = validate151;
const schema329 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1runs/get/responses/200/content/application~1json/schema"};
const schema330 = {"additionalProperties":false,"properties":{"items":{"items":{"additionalProperties":false,"properties":{"active_attempt_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"cancellation_requested_at":{"format":"date-time","type":["string","null"]},"current_attempt_no":{"format":"int64","maximum":4294967295,"minimum":0,"type":"integer"},"cycle_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"deadline_at":{"format":"date-time","type":"string"},"finished_at":{"format":"date-time","type":["string","null"]},"id":{"$ref":"#/components/schemas/Id"},"input_set_id":{"$ref":"#/components/schemas/Id"},"kind":{"$ref":"#/components/schemas/RunKind"},"last_event_seq":{"$ref":"#/components/schemas/DbCounter"},"project_id":{"$ref":"#/components/schemas/Id"},"queued_at":{"format":"date-time","type":"string"},"revision":{"$ref":"#/components/schemas/Revision"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"started_at":{"format":"date-time","type":["string","null"]},"state":{"$ref":"#/components/schemas/RunState"},"terminal_reason_code":{"type":["string","null"]}},"required":["schema_version","id","project_id","kind","input_set_id","state","current_attempt_no","last_event_seq","deadline_at","queued_at","revision"],"type":"object"},"type":"array"},"next_cursor":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","items"],"type":"object"};
const schema335 = {"enum":["AGENT_RESEARCH","DATA_VALIDATE","ALPHA_EVALUATE","PORTFOLIO_BUILD","PORTFOLIO_SIMULATE","FORWARD_EVALUATE","EXPORT","IMPORT"],"type":"string"};
const schema340 = {"enum":["QUEUED","DISPATCHING","RUNNING","RECONCILING","CANCEL_REQUESTED","SUCCEEDED","FAILED","CANCELLED"],"type":"string"};

function validate152(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate152.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.items === undefined) && (missing0 = "items"))){
validate152.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "items") || (key0 === "next_cursor")) || (key0 === "schema_version"))){
validate152.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.items !== undefined){
let data0 = data.items;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((((data1.schema_version === undefined) && (missing1 = "schema_version")) || ((data1.id === undefined) && (missing1 = "id"))) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.kind === undefined) && (missing1 = "kind"))) || ((data1.input_set_id === undefined) && (missing1 = "input_set_id"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.current_attempt_no === undefined) && (missing1 = "current_attempt_no"))) || ((data1.last_event_seq === undefined) && (missing1 = "last_event_seq"))) || ((data1.deadline_at === undefined) && (missing1 = "deadline_at"))) || ((data1.queued_at === undefined) && (missing1 = "queued_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema330.properties.items.items.properties, key1))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.active_attempt_id !== undefined){
let data2 = data1.active_attempt_id;
const _errs7 = errors;
const _errs8 = errors;
let valid3 = false;
let passing0 = null;
const _errs9 = errors;
if(data2 !== null){
const err0 = {instancePath:instancePath+"/items/" + i0+"/active_attempt_id",schemaPath:"#/properties/items/items/properties/active_attempt_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs9 === errors;
if(_valid0){
valid3 = true;
passing0 = 0;
}
const _errs11 = errors;
const _errs12 = errors;
if(errors === _errs12){
if(errors === _errs12){
if(typeof data2 === "string"){
if(!pattern5.test(data2)){
const err1 = {instancePath:instancePath+"/items/" + i0+"/active_attempt_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data2))){
const err2 = {instancePath:instancePath+"/items/" + i0+"/active_attempt_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/items/" + i0+"/active_attempt_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs11 === errors;
if(_valid0 && valid3){
valid3 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid3 = true;
passing0 = 1;
}
}
if(!valid3){
const err4 = {instancePath:instancePath+"/items/" + i0+"/active_attempt_id",schemaPath:"#/properties/items/items/properties/active_attempt_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate152.errors = vErrors;
return false;
}
else {
errors = _errs8;
if(vErrors !== null){
if(_errs8){
vErrors.length = _errs8;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs7 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.cancellation_requested_at !== undefined){
let data3 = data1.cancellation_requested_at;
const _errs14 = errors;
if((typeof data3 !== "string") && (data3 !== null)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/cancellation_requested_at",schemaPath:"#/properties/items/items/properties/cancellation_requested_at/type",keyword:"type",params:{type: schema330.properties.items.items.properties.cancellation_requested_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs14){
if(errors === _errs14){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/cancellation_requested_at",schemaPath:"#/properties/items/items/properties/cancellation_requested_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid2 = _errs14 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.current_attempt_no !== undefined){
let data4 = data1.current_attempt_no;
const _errs16 = errors;
if(!((typeof data4 == "number") && (!(data4 % 1) && !isNaN(data4)))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/current_attempt_no",schemaPath:"#/properties/items/items/properties/current_attempt_no/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs16){
if(typeof data4 == "number"){
if(data4 > 4294967295 || isNaN(data4)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/current_attempt_no",schemaPath:"#/properties/items/items/properties/current_attempt_no/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"}];
return false;
}
else {
if(data4 < 0 || isNaN(data4)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/current_attempt_no",schemaPath:"#/properties/items/items/properties/current_attempt_no/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
else {
if(!(formats96.validate(data4))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/current_attempt_no",schemaPath:"#/properties/items/items/properties/current_attempt_no/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""}];
return false;
}
}
}
}
}
var valid2 = _errs16 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.cycle_id !== undefined){
let data5 = data1.cycle_id;
const _errs18 = errors;
const _errs19 = errors;
let valid5 = false;
let passing1 = null;
const _errs20 = errors;
if(data5 !== null){
const err5 = {instancePath:instancePath+"/items/" + i0+"/cycle_id",schemaPath:"#/properties/items/items/properties/cycle_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs20 === errors;
if(_valid1){
valid5 = true;
passing1 = 0;
}
const _errs22 = errors;
const _errs23 = errors;
if(errors === _errs23){
if(errors === _errs23){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
const err6 = {instancePath:instancePath+"/items/" + i0+"/cycle_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data5))){
const err7 = {instancePath:instancePath+"/items/" + i0+"/cycle_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/items/" + i0+"/cycle_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs22 === errors;
if(_valid1 && valid5){
valid5 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid5 = true;
passing1 = 1;
}
}
if(!valid5){
const err9 = {instancePath:instancePath+"/items/" + i0+"/cycle_id",schemaPath:"#/properties/items/items/properties/cycle_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate152.errors = vErrors;
return false;
}
else {
errors = _errs19;
if(vErrors !== null){
if(_errs19){
vErrors.length = _errs19;
}
else {
vErrors = null;
}
}
}
var valid2 = _errs18 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.deadline_at !== undefined){
let data6 = data1.deadline_at;
const _errs25 = errors;
if(errors === _errs25){
if(errors === _errs25){
if(typeof data6 === "string"){
if(!(formats0.validate(data6))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/deadline_at",schemaPath:"#/properties/items/items/properties/deadline_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/deadline_at",schemaPath:"#/properties/items/items/properties/deadline_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs25 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.finished_at !== undefined){
let data7 = data1.finished_at;
const _errs27 = errors;
if((typeof data7 !== "string") && (data7 !== null)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/finished_at",schemaPath:"#/properties/items/items/properties/finished_at/type",keyword:"type",params:{type: schema330.properties.items.items.properties.finished_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs27){
if(errors === _errs27){
if(typeof data7 === "string"){
if(!(formats0.validate(data7))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/finished_at",schemaPath:"#/properties/items/items/properties/finished_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid2 = _errs27 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.id !== undefined){
let data8 = data1.id;
const _errs29 = errors;
const _errs30 = errors;
if(errors === _errs30){
if(errors === _errs30){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs29 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.input_set_id !== undefined){
let data9 = data1.input_set_id;
const _errs32 = errors;
const _errs33 = errors;
if(errors === _errs33){
if(errors === _errs33){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/input_set_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data9))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/input_set_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/input_set_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs32 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.kind !== undefined){
let data10 = data1.kind;
const _errs35 = errors;
if(typeof data10 !== "string"){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/kind",schemaPath:"#/components/schemas/RunKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((data10 === "AGENT_RESEARCH") || (data10 === "DATA_VALIDATE")) || (data10 === "ALPHA_EVALUATE")) || (data10 === "PORTFOLIO_BUILD")) || (data10 === "PORTFOLIO_SIMULATE")) || (data10 === "FORWARD_EVALUATE")) || (data10 === "EXPORT")) || (data10 === "IMPORT"))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/kind",schemaPath:"#/components/schemas/RunKind/enum",keyword:"enum",params:{allowedValues: schema335.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs35 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.last_event_seq !== undefined){
let data11 = data1.last_event_seq;
const _errs38 = errors;
const _errs39 = errors;
if(errors === _errs39){
if(typeof data11 === "string"){
if(func2(data11) > 19){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data11) < 1){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data11)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs38 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.project_id !== undefined){
let data12 = data1.project_id;
const _errs41 = errors;
const _errs42 = errors;
if(errors === _errs42){
if(errors === _errs42){
if(typeof data12 === "string"){
if(!pattern5.test(data12)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data12))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs41 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.queued_at !== undefined){
let data13 = data1.queued_at;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data13 === "string"){
if(!(formats0.validate(data13))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/queued_at",schemaPath:"#/properties/items/items/properties/queued_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/queued_at",schemaPath:"#/properties/items/items/properties/queued_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid2 = _errs44 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.revision !== undefined){
let data14 = data1.revision;
const _errs46 = errors;
const _errs47 = errors;
if(errors === _errs47){
if(typeof data14 === "string"){
if(func2(data14) > 19){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data14) < 1){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data14)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid2 = _errs46 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.schema_version !== undefined){
let data15 = data1.schema_version;
const _errs49 = errors;
const _errs50 = errors;
if(!((typeof data15 == "number") && (!(data15 % 1) && !isNaN(data15)))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data15 === 1)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs50){
if(typeof data15 == "number"){
if(data15 > 1 || isNaN(data15)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data15 < 1 || isNaN(data15)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid2 = _errs49 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.started_at !== undefined){
let data16 = data1.started_at;
const _errs52 = errors;
if((typeof data16 !== "string") && (data16 !== null)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/started_at",schemaPath:"#/properties/items/items/properties/started_at/type",keyword:"type",params:{type: schema330.properties.items.items.properties.started_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs52){
if(errors === _errs52){
if(typeof data16 === "string"){
if(!(formats0.validate(data16))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/started_at",schemaPath:"#/properties/items/items/properties/started_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid2 = _errs52 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.state !== undefined){
let data17 = data1.state;
const _errs54 = errors;
if(typeof data17 !== "string"){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/state",schemaPath:"#/components/schemas/RunState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((data17 === "QUEUED") || (data17 === "DISPATCHING")) || (data17 === "RUNNING")) || (data17 === "RECONCILING")) || (data17 === "CANCEL_REQUESTED")) || (data17 === "SUCCEEDED")) || (data17 === "FAILED")) || (data17 === "CANCELLED"))){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/state",schemaPath:"#/components/schemas/RunState/enum",keyword:"enum",params:{allowedValues: schema340.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid2 = _errs54 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data1.terminal_reason_code !== undefined){
let data18 = data1.terminal_reason_code;
const _errs57 = errors;
if((typeof data18 !== "string") && (data18 !== null)){
validate152.errors = [{instancePath:instancePath+"/items/" + i0+"/terminal_reason_code",schemaPath:"#/properties/items/items/properties/terminal_reason_code/type",keyword:"type",params:{type: schema330.properties.items.items.properties.terminal_reason_code.type},message:"must be string,null"}];
return false;
}
var valid2 = _errs57 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items/" + i0,schemaPath:"#/properties/items/items/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/items",schemaPath:"#/properties/items/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_cursor !== undefined){
let data19 = data.next_cursor;
const _errs59 = errors;
const _errs60 = errors;
let valid15 = false;
let passing2 = null;
const _errs61 = errors;
if(data19 !== null){
const err10 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
var _valid2 = _errs61 === errors;
if(_valid2){
valid15 = true;
passing2 = 0;
}
const _errs63 = errors;
const _errs64 = errors;
if(errors === _errs64){
if(errors === _errs64){
if(typeof data19 === "string"){
if(!pattern5.test(data19)){
const err11 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
else {
if(!(formats2.test(data19))){
const err12 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
else {
const err13 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
}
var _valid2 = _errs63 === errors;
if(_valid2 && valid15){
valid15 = false;
passing2 = [passing2, 1];
}
else {
if(_valid2){
valid15 = true;
passing2 = 1;
}
}
if(!valid15){
const err14 = {instancePath:instancePath+"/next_cursor",schemaPath:"#/properties/next_cursor/oneOf",keyword:"oneOf",params:{passingSchemas: passing2},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
validate152.errors = vErrors;
return false;
}
else {
errors = _errs60;
if(vErrors !== null){
if(_errs60){
vErrors.length = _errs60;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs59 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data20 = data.schema_version;
const _errs66 = errors;
const _errs67 = errors;
if(!((typeof data20 == "number") && (!(data20 % 1) && !isNaN(data20)))){
validate152.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data20 === 1)){
validate152.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs67){
if(typeof data20 == "number"){
if(data20 > 1 || isNaN(data20)){
validate152.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data20 < 1 || isNaN(data20)){
validate152.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs66 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate152.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate152.errors = vErrors;
return errors === 0;
}
validate152.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate151(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate151.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate152(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate152.errors : vErrors.concat(validate152.errors);
errors = vErrors.length;
}
validate151.errors = vErrors;
return errors === 0;
}
validate151.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response33 = validate154;
const schema343 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1runs~1{id}/get/responses/200/content/application~1json/schema"};
const schema344 = {"additionalProperties":false,"properties":{"active_attempt_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"cancellation_requested_at":{"format":"date-time","type":["string","null"]},"current_attempt_no":{"format":"int64","maximum":4294967295,"minimum":0,"type":"integer"},"cycle_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"deadline_at":{"format":"date-time","type":"string"},"finished_at":{"format":"date-time","type":["string","null"]},"id":{"$ref":"#/components/schemas/Id"},"input_set_id":{"$ref":"#/components/schemas/Id"},"kind":{"$ref":"#/components/schemas/RunKind"},"last_event_seq":{"$ref":"#/components/schemas/DbCounter"},"project_id":{"$ref":"#/components/schemas/Id"},"queued_at":{"format":"date-time","type":"string"},"revision":{"$ref":"#/components/schemas/Revision"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"started_at":{"format":"date-time","type":["string","null"]},"state":{"$ref":"#/components/schemas/RunState"},"terminal_reason_code":{"type":["string","null"]}},"required":["schema_version","id","project_id","kind","input_set_id","state","current_attempt_no","last_event_seq","deadline_at","queued_at","revision"],"type":"object"};

function validate155(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate155.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.id === undefined) && (missing0 = "id"))) || ((data.project_id === undefined) && (missing0 = "project_id"))) || ((data.kind === undefined) && (missing0 = "kind"))) || ((data.input_set_id === undefined) && (missing0 = "input_set_id"))) || ((data.state === undefined) && (missing0 = "state"))) || ((data.current_attempt_no === undefined) && (missing0 = "current_attempt_no"))) || ((data.last_event_seq === undefined) && (missing0 = "last_event_seq"))) || ((data.deadline_at === undefined) && (missing0 = "deadline_at"))) || ((data.queued_at === undefined) && (missing0 = "queued_at"))) || ((data.revision === undefined) && (missing0 = "revision"))){
validate155.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(func1.call(schema344.properties, key0))){
validate155.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.active_attempt_id !== undefined){
let data0 = data.active_attempt_id;
const _errs2 = errors;
const _errs3 = errors;
let valid1 = false;
let passing0 = null;
const _errs4 = errors;
if(data0 !== null){
const err0 = {instancePath:instancePath+"/active_attempt_id",schemaPath:"#/properties/active_attempt_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs4 === errors;
if(_valid0){
valid1 = true;
passing0 = 0;
}
const _errs6 = errors;
const _errs7 = errors;
if(errors === _errs7){
if(errors === _errs7){
if(typeof data0 === "string"){
if(!pattern5.test(data0)){
const err1 = {instancePath:instancePath+"/active_attempt_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data0))){
const err2 = {instancePath:instancePath+"/active_attempt_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/active_attempt_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs6 === errors;
if(_valid0 && valid1){
valid1 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid1 = true;
passing0 = 1;
}
}
if(!valid1){
const err4 = {instancePath:instancePath+"/active_attempt_id",schemaPath:"#/properties/active_attempt_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate155.errors = vErrors;
return false;
}
else {
errors = _errs3;
if(vErrors !== null){
if(_errs3){
vErrors.length = _errs3;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.cancellation_requested_at !== undefined){
let data1 = data.cancellation_requested_at;
const _errs9 = errors;
if((typeof data1 !== "string") && (data1 !== null)){
validate155.errors = [{instancePath:instancePath+"/cancellation_requested_at",schemaPath:"#/properties/cancellation_requested_at/type",keyword:"type",params:{type: schema344.properties.cancellation_requested_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs9){
if(errors === _errs9){
if(typeof data1 === "string"){
if(!(formats0.validate(data1))){
validate155.errors = [{instancePath:instancePath+"/cancellation_requested_at",schemaPath:"#/properties/cancellation_requested_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.current_attempt_no !== undefined){
let data2 = data.current_attempt_no;
const _errs11 = errors;
if(!((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2)))){
validate155.errors = [{instancePath:instancePath+"/current_attempt_no",schemaPath:"#/properties/current_attempt_no/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs11){
if(typeof data2 == "number"){
if(data2 > 4294967295 || isNaN(data2)){
validate155.errors = [{instancePath:instancePath+"/current_attempt_no",schemaPath:"#/properties/current_attempt_no/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"}];
return false;
}
else {
if(data2 < 0 || isNaN(data2)){
validate155.errors = [{instancePath:instancePath+"/current_attempt_no",schemaPath:"#/properties/current_attempt_no/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
else {
if(!(formats96.validate(data2))){
validate155.errors = [{instancePath:instancePath+"/current_attempt_no",schemaPath:"#/properties/current_attempt_no/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""}];
return false;
}
}
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.cycle_id !== undefined){
let data3 = data.cycle_id;
const _errs13 = errors;
const _errs14 = errors;
let valid3 = false;
let passing1 = null;
const _errs15 = errors;
if(data3 !== null){
const err5 = {instancePath:instancePath+"/cycle_id",schemaPath:"#/properties/cycle_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs15 === errors;
if(_valid1){
valid3 = true;
passing1 = 0;
}
const _errs17 = errors;
const _errs18 = errors;
if(errors === _errs18){
if(errors === _errs18){
if(typeof data3 === "string"){
if(!pattern5.test(data3)){
const err6 = {instancePath:instancePath+"/cycle_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data3))){
const err7 = {instancePath:instancePath+"/cycle_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/cycle_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs17 === errors;
if(_valid1 && valid3){
valid3 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid3 = true;
passing1 = 1;
}
}
if(!valid3){
const err9 = {instancePath:instancePath+"/cycle_id",schemaPath:"#/properties/cycle_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate155.errors = vErrors;
return false;
}
else {
errors = _errs14;
if(vErrors !== null){
if(_errs14){
vErrors.length = _errs14;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.deadline_at !== undefined){
let data4 = data.deadline_at;
const _errs20 = errors;
if(errors === _errs20){
if(errors === _errs20){
if(typeof data4 === "string"){
if(!(formats0.validate(data4))){
validate155.errors = [{instancePath:instancePath+"/deadline_at",schemaPath:"#/properties/deadline_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate155.errors = [{instancePath:instancePath+"/deadline_at",schemaPath:"#/properties/deadline_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs20 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.finished_at !== undefined){
let data5 = data.finished_at;
const _errs22 = errors;
if((typeof data5 !== "string") && (data5 !== null)){
validate155.errors = [{instancePath:instancePath+"/finished_at",schemaPath:"#/properties/finished_at/type",keyword:"type",params:{type: schema344.properties.finished_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs22){
if(errors === _errs22){
if(typeof data5 === "string"){
if(!(formats0.validate(data5))){
validate155.errors = [{instancePath:instancePath+"/finished_at",schemaPath:"#/properties/finished_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs22 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data6 = data.id;
const _errs24 = errors;
const _errs25 = errors;
if(errors === _errs25){
if(errors === _errs25){
if(typeof data6 === "string"){
if(!pattern5.test(data6)){
validate155.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data6))){
validate155.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate155.errors = [{instancePath:instancePath+"/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs24 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.input_set_id !== undefined){
let data7 = data.input_set_id;
const _errs27 = errors;
const _errs28 = errors;
if(errors === _errs28){
if(errors === _errs28){
if(typeof data7 === "string"){
if(!pattern5.test(data7)){
validate155.errors = [{instancePath:instancePath+"/input_set_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data7))){
validate155.errors = [{instancePath:instancePath+"/input_set_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate155.errors = [{instancePath:instancePath+"/input_set_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs27 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.kind !== undefined){
let data8 = data.kind;
const _errs30 = errors;
if(typeof data8 !== "string"){
validate155.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/components/schemas/RunKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((data8 === "AGENT_RESEARCH") || (data8 === "DATA_VALIDATE")) || (data8 === "ALPHA_EVALUATE")) || (data8 === "PORTFOLIO_BUILD")) || (data8 === "PORTFOLIO_SIMULATE")) || (data8 === "FORWARD_EVALUATE")) || (data8 === "EXPORT")) || (data8 === "IMPORT"))){
validate155.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/components/schemas/RunKind/enum",keyword:"enum",params:{allowedValues: schema335.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs30 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.last_event_seq !== undefined){
let data9 = data.last_event_seq;
const _errs33 = errors;
const _errs34 = errors;
if(errors === _errs34){
if(typeof data9 === "string"){
if(func2(data9) > 19){
validate155.errors = [{instancePath:instancePath+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data9) < 1){
validate155.errors = [{instancePath:instancePath+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data9)){
validate155.errors = [{instancePath:instancePath+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate155.errors = [{instancePath:instancePath+"/last_event_seq",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs33 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.project_id !== undefined){
let data10 = data.project_id;
const _errs36 = errors;
const _errs37 = errors;
if(errors === _errs37){
if(errors === _errs37){
if(typeof data10 === "string"){
if(!pattern5.test(data10)){
validate155.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data10))){
validate155.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate155.errors = [{instancePath:instancePath+"/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs36 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.queued_at !== undefined){
let data11 = data.queued_at;
const _errs39 = errors;
if(errors === _errs39){
if(errors === _errs39){
if(typeof data11 === "string"){
if(!(formats0.validate(data11))){
validate155.errors = [{instancePath:instancePath+"/queued_at",schemaPath:"#/properties/queued_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate155.errors = [{instancePath:instancePath+"/queued_at",schemaPath:"#/properties/queued_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid0 = _errs39 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revision !== undefined){
let data12 = data.revision;
const _errs41 = errors;
const _errs42 = errors;
if(errors === _errs42){
if(typeof data12 === "string"){
if(func2(data12) > 19){
validate155.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data12) < 1){
validate155.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data12)){
validate155.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate155.errors = [{instancePath:instancePath+"/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs41 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data13 = data.schema_version;
const _errs44 = errors;
const _errs45 = errors;
if(!((typeof data13 == "number") && (!(data13 % 1) && !isNaN(data13)))){
validate155.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data13 === 1)){
validate155.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs45){
if(typeof data13 == "number"){
if(data13 > 1 || isNaN(data13)){
validate155.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data13 < 1 || isNaN(data13)){
validate155.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs44 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.started_at !== undefined){
let data14 = data.started_at;
const _errs47 = errors;
if((typeof data14 !== "string") && (data14 !== null)){
validate155.errors = [{instancePath:instancePath+"/started_at",schemaPath:"#/properties/started_at/type",keyword:"type",params:{type: schema344.properties.started_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs47){
if(errors === _errs47){
if(typeof data14 === "string"){
if(!(formats0.validate(data14))){
validate155.errors = [{instancePath:instancePath+"/started_at",schemaPath:"#/properties/started_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid0 = _errs47 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.state !== undefined){
let data15 = data.state;
const _errs49 = errors;
if(typeof data15 !== "string"){
validate155.errors = [{instancePath:instancePath+"/state",schemaPath:"#/components/schemas/RunState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((data15 === "QUEUED") || (data15 === "DISPATCHING")) || (data15 === "RUNNING")) || (data15 === "RECONCILING")) || (data15 === "CANCEL_REQUESTED")) || (data15 === "SUCCEEDED")) || (data15 === "FAILED")) || (data15 === "CANCELLED"))){
validate155.errors = [{instancePath:instancePath+"/state",schemaPath:"#/components/schemas/RunState/enum",keyword:"enum",params:{allowedValues: schema340.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid0 = _errs49 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.terminal_reason_code !== undefined){
let data16 = data.terminal_reason_code;
const _errs52 = errors;
if((typeof data16 !== "string") && (data16 !== null)){
validate155.errors = [{instancePath:instancePath+"/terminal_reason_code",schemaPath:"#/properties/terminal_reason_code/type",keyword:"type",params:{type: schema344.properties.terminal_reason_code.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs52 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate155.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate155.errors = vErrors;
return errors === 0;
}
validate155.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate154(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate154.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate155(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate155.errors : vErrors.concat(validate155.errors);
errors = vErrors.length;
}
validate154.errors = vErrors;
return errors === 0;
}
validate154.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.response34 = validate157;
const schema355 = {"$ref":"https://contracts.quazonai.invalid/api-v2#/paths/~1api~1v2~1runs~1{id}~1cancel/post/responses/202/content/application~1json/schema"};
const schema356 = {"additionalProperties":false,"properties":{"replayed":{"type":"boolean"},"resource":{"additionalProperties":false,"properties":{"active_attempt_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"cancellation_requested_at":{"format":"date-time","type":["string","null"]},"current_attempt_no":{"format":"int64","maximum":4294967295,"minimum":0,"type":"integer"},"cycle_id":{"oneOf":[{"type":"null"},{"$ref":"#/components/schemas/Id"}]},"deadline_at":{"format":"date-time","type":"string"},"finished_at":{"format":"date-time","type":["string","null"]},"id":{"$ref":"#/components/schemas/Id"},"input_set_id":{"$ref":"#/components/schemas/Id"},"kind":{"$ref":"#/components/schemas/RunKind"},"last_event_seq":{"$ref":"#/components/schemas/DbCounter"},"project_id":{"$ref":"#/components/schemas/Id"},"queued_at":{"format":"date-time","type":"string"},"revision":{"$ref":"#/components/schemas/Revision"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"},"started_at":{"format":"date-time","type":["string","null"]},"state":{"$ref":"#/components/schemas/RunState"},"terminal_reason_code":{"type":["string","null"]}},"required":["schema_version","id","project_id","kind","input_set_id","state","current_attempt_no","last_event_seq","deadline_at","queued_at","revision"],"type":"object"},"schema_version":{"$ref":"#/components/schemas/SchemaV1"}},"required":["schema_version","replayed","resource"],"type":"object"};

function validate158(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate158.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.replayed === undefined) && (missing0 = "replayed"))) || ((data.resource === undefined) && (missing0 = "resource"))){
validate158.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "replayed") || (key0 === "resource")) || (key0 === "schema_version"))){
validate158.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.replayed !== undefined){
const _errs2 = errors;
if(typeof data.replayed !== "boolean"){
validate158.errors = [{instancePath:instancePath+"/replayed",schemaPath:"#/properties/replayed/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.resource !== undefined){
let data1 = data.resource;
const _errs4 = errors;
if(errors === _errs4){
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
let missing1;
if((((((((((((data1.schema_version === undefined) && (missing1 = "schema_version")) || ((data1.id === undefined) && (missing1 = "id"))) || ((data1.project_id === undefined) && (missing1 = "project_id"))) || ((data1.kind === undefined) && (missing1 = "kind"))) || ((data1.input_set_id === undefined) && (missing1 = "input_set_id"))) || ((data1.state === undefined) && (missing1 = "state"))) || ((data1.current_attempt_no === undefined) && (missing1 = "current_attempt_no"))) || ((data1.last_event_seq === undefined) && (missing1 = "last_event_seq"))) || ((data1.deadline_at === undefined) && (missing1 = "deadline_at"))) || ((data1.queued_at === undefined) && (missing1 = "queued_at"))) || ((data1.revision === undefined) && (missing1 = "revision"))){
validate158.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"}];
return false;
}
else {
const _errs6 = errors;
for(const key1 in data1){
if(!(func1.call(schema356.properties.resource.properties, key1))){
validate158.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs6 === errors){
if(data1.active_attempt_id !== undefined){
let data2 = data1.active_attempt_id;
const _errs7 = errors;
const _errs8 = errors;
let valid2 = false;
let passing0 = null;
const _errs9 = errors;
if(data2 !== null){
const err0 = {instancePath:instancePath+"/resource/active_attempt_id",schemaPath:"#/properties/resource/properties/active_attempt_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs9 === errors;
if(_valid0){
valid2 = true;
passing0 = 0;
}
const _errs11 = errors;
const _errs12 = errors;
if(errors === _errs12){
if(errors === _errs12){
if(typeof data2 === "string"){
if(!pattern5.test(data2)){
const err1 = {instancePath:instancePath+"/resource/active_attempt_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
else {
if(!(formats2.test(data2))){
const err2 = {instancePath:instancePath+"/resource/active_attempt_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
}
else {
const err3 = {instancePath:instancePath+"/resource/active_attempt_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
var _valid0 = _errs11 === errors;
if(_valid0 && valid2){
valid2 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid2 = true;
passing0 = 1;
}
}
if(!valid2){
const err4 = {instancePath:instancePath+"/resource/active_attempt_id",schemaPath:"#/properties/resource/properties/active_attempt_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
validate158.errors = vErrors;
return false;
}
else {
errors = _errs8;
if(vErrors !== null){
if(_errs8){
vErrors.length = _errs8;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs7 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.cancellation_requested_at !== undefined){
let data3 = data1.cancellation_requested_at;
const _errs14 = errors;
if((typeof data3 !== "string") && (data3 !== null)){
validate158.errors = [{instancePath:instancePath+"/resource/cancellation_requested_at",schemaPath:"#/properties/resource/properties/cancellation_requested_at/type",keyword:"type",params:{type: schema356.properties.resource.properties.cancellation_requested_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs14){
if(errors === _errs14){
if(typeof data3 === "string"){
if(!(formats0.validate(data3))){
validate158.errors = [{instancePath:instancePath+"/resource/cancellation_requested_at",schemaPath:"#/properties/resource/properties/cancellation_requested_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs14 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.current_attempt_no !== undefined){
let data4 = data1.current_attempt_no;
const _errs16 = errors;
if(!((typeof data4 == "number") && (!(data4 % 1) && !isNaN(data4)))){
validate158.errors = [{instancePath:instancePath+"/resource/current_attempt_no",schemaPath:"#/properties/resource/properties/current_attempt_no/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs16){
if(typeof data4 == "number"){
if(data4 > 4294967295 || isNaN(data4)){
validate158.errors = [{instancePath:instancePath+"/resource/current_attempt_no",schemaPath:"#/properties/resource/properties/current_attempt_no/maximum",keyword:"maximum",params:{comparison: "<=", limit: 4294967295},message:"must be <= 4294967295"}];
return false;
}
else {
if(data4 < 0 || isNaN(data4)){
validate158.errors = [{instancePath:instancePath+"/resource/current_attempt_no",schemaPath:"#/properties/resource/properties/current_attempt_no/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
else {
if(!(formats96.validate(data4))){
validate158.errors = [{instancePath:instancePath+"/resource/current_attempt_no",schemaPath:"#/properties/resource/properties/current_attempt_no/format",keyword:"format",params:{format: "int64"},message:"must match format \""+"int64"+"\""}];
return false;
}
}
}
}
}
var valid1 = _errs16 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.cycle_id !== undefined){
let data5 = data1.cycle_id;
const _errs18 = errors;
const _errs19 = errors;
let valid4 = false;
let passing1 = null;
const _errs20 = errors;
if(data5 !== null){
const err5 = {instancePath:instancePath+"/resource/cycle_id",schemaPath:"#/properties/resource/properties/cycle_id/oneOf/0/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
var _valid1 = _errs20 === errors;
if(_valid1){
valid4 = true;
passing1 = 0;
}
const _errs22 = errors;
const _errs23 = errors;
if(errors === _errs23){
if(errors === _errs23){
if(typeof data5 === "string"){
if(!pattern5.test(data5)){
const err6 = {instancePath:instancePath+"/resource/cycle_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
else {
if(!(formats2.test(data5))){
const err7 = {instancePath:instancePath+"/resource/cycle_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
}
}
else {
const err8 = {instancePath:instancePath+"/resource/cycle_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
}
var _valid1 = _errs22 === errors;
if(_valid1 && valid4){
valid4 = false;
passing1 = [passing1, 1];
}
else {
if(_valid1){
valid4 = true;
passing1 = 1;
}
}
if(!valid4){
const err9 = {instancePath:instancePath+"/resource/cycle_id",schemaPath:"#/properties/resource/properties/cycle_id/oneOf",keyword:"oneOf",params:{passingSchemas: passing1},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
validate158.errors = vErrors;
return false;
}
else {
errors = _errs19;
if(vErrors !== null){
if(_errs19){
vErrors.length = _errs19;
}
else {
vErrors = null;
}
}
}
var valid1 = _errs18 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.deadline_at !== undefined){
let data6 = data1.deadline_at;
const _errs25 = errors;
if(errors === _errs25){
if(errors === _errs25){
if(typeof data6 === "string"){
if(!(formats0.validate(data6))){
validate158.errors = [{instancePath:instancePath+"/resource/deadline_at",schemaPath:"#/properties/resource/properties/deadline_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource/deadline_at",schemaPath:"#/properties/resource/properties/deadline_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs25 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.finished_at !== undefined){
let data7 = data1.finished_at;
const _errs27 = errors;
if((typeof data7 !== "string") && (data7 !== null)){
validate158.errors = [{instancePath:instancePath+"/resource/finished_at",schemaPath:"#/properties/resource/properties/finished_at/type",keyword:"type",params:{type: schema356.properties.resource.properties.finished_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs27){
if(errors === _errs27){
if(typeof data7 === "string"){
if(!(formats0.validate(data7))){
validate158.errors = [{instancePath:instancePath+"/resource/finished_at",schemaPath:"#/properties/resource/properties/finished_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs27 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.id !== undefined){
let data8 = data1.id;
const _errs29 = errors;
const _errs30 = errors;
if(errors === _errs30){
if(errors === _errs30){
if(typeof data8 === "string"){
if(!pattern5.test(data8)){
validate158.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data8))){
validate158.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource/id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs29 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.input_set_id !== undefined){
let data9 = data1.input_set_id;
const _errs32 = errors;
const _errs33 = errors;
if(errors === _errs33){
if(errors === _errs33){
if(typeof data9 === "string"){
if(!pattern5.test(data9)){
validate158.errors = [{instancePath:instancePath+"/resource/input_set_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data9))){
validate158.errors = [{instancePath:instancePath+"/resource/input_set_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource/input_set_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs32 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.kind !== undefined){
let data10 = data1.kind;
const _errs35 = errors;
if(typeof data10 !== "string"){
validate158.errors = [{instancePath:instancePath+"/resource/kind",schemaPath:"#/components/schemas/RunKind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((data10 === "AGENT_RESEARCH") || (data10 === "DATA_VALIDATE")) || (data10 === "ALPHA_EVALUATE")) || (data10 === "PORTFOLIO_BUILD")) || (data10 === "PORTFOLIO_SIMULATE")) || (data10 === "FORWARD_EVALUATE")) || (data10 === "EXPORT")) || (data10 === "IMPORT"))){
validate158.errors = [{instancePath:instancePath+"/resource/kind",schemaPath:"#/components/schemas/RunKind/enum",keyword:"enum",params:{allowedValues: schema335.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs35 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.last_event_seq !== undefined){
let data11 = data1.last_event_seq;
const _errs38 = errors;
const _errs39 = errors;
if(errors === _errs39){
if(typeof data11 === "string"){
if(func2(data11) > 19){
validate158.errors = [{instancePath:instancePath+"/resource/last_event_seq",schemaPath:"#/components/schemas/DbCounter/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data11) < 1){
validate158.errors = [{instancePath:instancePath+"/resource/last_event_seq",schemaPath:"#/components/schemas/DbCounter/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern4.test(data11)){
validate158.errors = [{instancePath:instancePath+"/resource/last_event_seq",schemaPath:"#/components/schemas/DbCounter/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|0|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource/last_event_seq",schemaPath:"#/components/schemas/DbCounter/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs38 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.project_id !== undefined){
let data12 = data1.project_id;
const _errs41 = errors;
const _errs42 = errors;
if(errors === _errs42){
if(errors === _errs42){
if(typeof data12 === "string"){
if(!pattern5.test(data12)){
validate158.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/pattern",keyword:"pattern",params:{pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"},message:"must match pattern \""+"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"+"\""}];
return false;
}
else {
if(!(formats2.test(data12))){
validate158.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/format",keyword:"format",params:{format: "uuid"},message:"must match format \""+"uuid"+"\""}];
return false;
}
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource/project_id",schemaPath:"#/components/schemas/Id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs41 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.queued_at !== undefined){
let data13 = data1.queued_at;
const _errs44 = errors;
if(errors === _errs44){
if(errors === _errs44){
if(typeof data13 === "string"){
if(!(formats0.validate(data13))){
validate158.errors = [{instancePath:instancePath+"/resource/queued_at",schemaPath:"#/properties/resource/properties/queued_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource/queued_at",schemaPath:"#/properties/resource/properties/queued_at/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
var valid1 = _errs44 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.revision !== undefined){
let data14 = data1.revision;
const _errs46 = errors;
const _errs47 = errors;
if(errors === _errs47){
if(typeof data14 === "string"){
if(func2(data14) > 19){
validate158.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/maxLength",keyword:"maxLength",params:{limit: 19},message:"must NOT have more than 19 characters"}];
return false;
}
else {
if(func2(data14) < 1){
validate158.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
else {
if(!pattern27.test(data14)){
validate158.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/pattern",keyword:"pattern",params:{pattern: "^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"},message:"must match pattern \""+"^(?:[1-9][0-9]{0,17}|[1-8][0-9]{18}|9[0-1][0-9]{17}|92[0-1][0-9]{16}|922[0-2][0-9]{15}|9223[0-2][0-9]{14}|92233[0-6][0-9]{13}|922337[0-1][0-9]{12}|92233720[0-2][0-9]{10}|922337203[0-5][0-9]{9}|9223372036[0-7][0-9]{8}|92233720368[0-4][0-9]{7}|922337203685[0-3][0-9]{6}|9223372036854[0-6][0-9]{5}|92233720368547[0-6][0-9]{4}|922337203685477[0-4][0-9]{3}|9223372036854775[0-7][0-9]{2}|922337203685477580[0-6][0-9]{0}|9223372036854775807)(?![\\s\\S])"+"\""}];
return false;
}
}
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource/revision",schemaPath:"#/components/schemas/Revision/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid1 = _errs46 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.schema_version !== undefined){
let data15 = data1.schema_version;
const _errs49 = errors;
const _errs50 = errors;
if(!((typeof data15 == "number") && (!(data15 % 1) && !isNaN(data15)))){
validate158.errors = [{instancePath:instancePath+"/resource/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data15 === 1)){
validate158.errors = [{instancePath:instancePath+"/resource/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs50){
if(typeof data15 == "number"){
if(data15 > 1 || isNaN(data15)){
validate158.errors = [{instancePath:instancePath+"/resource/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data15 < 1 || isNaN(data15)){
validate158.errors = [{instancePath:instancePath+"/resource/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid1 = _errs49 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.started_at !== undefined){
let data16 = data1.started_at;
const _errs52 = errors;
if((typeof data16 !== "string") && (data16 !== null)){
validate158.errors = [{instancePath:instancePath+"/resource/started_at",schemaPath:"#/properties/resource/properties/started_at/type",keyword:"type",params:{type: schema356.properties.resource.properties.started_at.type},message:"must be string,null"}];
return false;
}
if(errors === _errs52){
if(errors === _errs52){
if(typeof data16 === "string"){
if(!(formats0.validate(data16))){
validate158.errors = [{instancePath:instancePath+"/resource/started_at",schemaPath:"#/properties/resource/properties/started_at/format",keyword:"format",params:{format: "date-time"},message:"must match format \""+"date-time"+"\""}];
return false;
}
}
}
}
var valid1 = _errs52 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.state !== undefined){
let data17 = data1.state;
const _errs54 = errors;
if(typeof data17 !== "string"){
validate158.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/RunState/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!((((((((data17 === "QUEUED") || (data17 === "DISPATCHING")) || (data17 === "RUNNING")) || (data17 === "RECONCILING")) || (data17 === "CANCEL_REQUESTED")) || (data17 === "SUCCEEDED")) || (data17 === "FAILED")) || (data17 === "CANCELLED"))){
validate158.errors = [{instancePath:instancePath+"/resource/state",schemaPath:"#/components/schemas/RunState/enum",keyword:"enum",params:{allowedValues: schema340.enum},message:"must be equal to one of the allowed values"}];
return false;
}
var valid1 = _errs54 === errors;
}
else {
var valid1 = true;
}
if(valid1){
if(data1.terminal_reason_code !== undefined){
let data18 = data1.terminal_reason_code;
const _errs57 = errors;
if((typeof data18 !== "string") && (data18 !== null)){
validate158.errors = [{instancePath:instancePath+"/resource/terminal_reason_code",schemaPath:"#/properties/resource/properties/terminal_reason_code/type",keyword:"type",params:{type: schema356.properties.resource.properties.terminal_reason_code.type},message:"must be string,null"}];
return false;
}
var valid1 = _errs57 === errors;
}
else {
var valid1 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate158.errors = [{instancePath:instancePath+"/resource",schemaPath:"#/properties/resource/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data19 = data.schema_version;
const _errs59 = errors;
const _errs60 = errors;
if(!((typeof data19 == "number") && (!(data19 % 1) && !isNaN(data19)))){
validate158.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(!(data19 === 1)){
validate158.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"}];
return false;
}
if(errors === _errs60){
if(typeof data19 == "number"){
if(data19 > 1 || isNaN(data19)){
validate158.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/maximum",keyword:"maximum",params:{comparison: "<=", limit: 1},message:"must be <= 1"}];
return false;
}
else {
if(data19 < 1 || isNaN(data19)){
validate158.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/components/schemas/SchemaV1/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs59 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate158.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate158.errors = vErrors;
return errors === 0;
}
validate158.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate157(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate157.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(!(validate158(data, {instancePath,parentData,parentDataProperty,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate158.errors : vErrors.concat(validate158.errors);
errors = vErrors.length;
}
validate157.errors = vErrors;
return errors === 0;
}
validate157.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

const responses = {
  "GET /api/v2/artifacts 200": "response0",
  "POST /api/v2/artifacts 201": "response1",
  "GET /api/v2/artifacts/{id} 200": "response2",
  "GET /api/v2/auth/devices 200": "response3",
  "DELETE /api/v2/auth/devices/{id} 204": null,
  "POST /api/v2/auth/login 200": "response4",
  "POST /api/v2/auth/logout 204": null,
  "GET /api/v2/auth/machine 200": "response5",
  "POST /api/v2/auth/operator-command-grants 201": "response6",
  "GET /api/v2/auth/session 200": "response7",
  "POST /api/v2/auth/verify 200": "response8",
  "POST /api/v2/bootstrap/confirm 200": "response9",
  "POST /api/v2/bootstrap/start 201": "response10",
  "GET /api/v2/bootstrap/status 200": "response11",
  "GET /api/v2/briefs/{id} 200": "response12",
  "PATCH /api/v2/briefs/{id} 200": "response13",
  "GET /api/v2/evaluation-policies 200": "response14",
  "POST /api/v2/evaluation-policies 201": "response15",
  "GET /api/v2/evaluation-policies/{id} 200": "response16",
  "GET /api/v2/input-sets 200": "response17",
  "POST /api/v2/input-sets 201": "response18",
  "GET /api/v2/input-sets/{id} 200": "response19",
  "POST /api/v2/machine-credentials/{id}/revoke 200": "response20",
  "GET /api/v2/machine-principals 200": "response21",
  "POST /api/v2/machine-principals 201": "response22",
  "PATCH /api/v2/machine-principals/{id} 200": "response23",
  "GET /api/v2/machine-principals/{id}/credentials 200": "response24",
  "POST /api/v2/machine-principals/{id}/credentials 201": "response25",
  "GET /api/v2/projects 200": "response26",
  "POST /api/v2/projects 201": "response27",
  "GET /api/v2/projects/{id} 200": "response28",
  "PATCH /api/v2/projects/{id} 200": "response29",
  "GET /api/v2/projects/{id}/briefs 200": "response30",
  "POST /api/v2/projects/{id}/briefs 201": "response31",
  "GET /api/v2/runs 200": "response32",
  "GET /api/v2/runs/{id} 200": "response33",
  "POST /api/v2/runs/{id}/cancel 202": "response34"
};
exports.validateResponse = function(path, method, status, value) {
  const key = method + ' ' + path + ' ' + status;
  if (!Object.prototype.hasOwnProperty.call(responses, key)) return false;
  const name = responses[key];
  return name === null ? value === undefined : exports[name](value);
};
