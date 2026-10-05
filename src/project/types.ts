export interface PackageJsonSummary{name?:string;version?:string;scripts:Record<string,string>;dependencies:string[];devDependencies:string[]}
export interface ProjectInfo{name:string;projectTypes:string[];languages:string[];detectedFrameworks:string[];packageManager?:string;hasGit:boolean;mainConfigFiles:string[];package?:PackageJsonSummary}
export interface ProjectMap{mainDirectories:string[];configFiles:string[];filesByExtension:Record<string,number>;entryPoints:string[];hasTests:boolean;totalFiles:number;truncated:boolean}
export interface WorkspaceInfo{name:string;root:string;projectInfo:ProjectInfo;projectMap:ProjectMap}
export interface RecentProject{displayName:string;path:string;lastOpened:number}
export interface ProjectTreeNode{name:string;relativePath:string;kind:"file"|"directory";extension?:string;size:number;modifiedAt?:number;isIgnored:boolean;isSymlink:boolean}
export type FileContent={kind:"text";content:string}|{kind:"binary"};
export interface FileStat{size:number;modifiedAt:number;readOnly:boolean}
