// @vitest-environment jsdom
import{beforeEach,describe,expect,it,vi}from"vitest";
const api=vi.hoisted(()=>({read:vi.fn(),stat:vi.fn(),write:vi.fn()}));
vi.mock("../project/workspaceApi",()=>({workspaceApi:{...api}}));
import{useEditorStore}from"./editorStore";

describe("editor state",()=>{beforeEach(()=>{const values=new Map<string,string>();vi.stubGlobal("localStorage",{getItem:(key:string)=>values.get(key)??null,setItem:(key:string,value:string)=>values.set(key,value),removeItem:(key:string)=>values.delete(key),clear:()=>values.clear()});api.read.mockResolvedValue({kind:"text",content:"const a = 1;"});api.stat.mockResolvedValue({size:12,modifiedAt:1,readOnly:false});api.write.mockResolvedValue({size:12,modifiedAt:2,readOnly:false});useEditorStore.setState({tabs:[],activePath:null,diffPath:null,error:null});});
 it("opens and switches multiple tabs",async()=>{await useEditorStore.getState().open("src/a.ts");await useEditorStore.getState().open("src/b.ts");expect(useEditorStore.getState().tabs).toHaveLength(2);useEditorStore.getState().activate("src/a.ts");expect(useEditorStore.getState().activePath).toBe("src/a.ts");});
 it("tracks dirty state and saves",async()=>{await useEditorStore.getState().open("src/a.ts");useEditorStore.getState().change("src/a.ts","changed");expect(useEditorStore.getState().tabs[0]?.dirty).toBe(true);await useEditorStore.getState().save();expect(api.write).toHaveBeenCalledWith("src/a.ts","changed");expect(useEditorStore.getState().tabs[0]?.dirty).toBe(false);});
 it("blocks dirty close and exposes diff state",async()=>{await useEditorStore.getState().open("src/a.ts");useEditorStore.getState().change("src/a.ts","changed");useEditorStore.getState().viewDiff("src/a.ts");expect(useEditorStore.getState().diffPath).toBe("src/a.ts");expect(useEditorStore.getState().close("src/a.ts")).toBe(false);expect(useEditorStore.getState().close("src/a.ts",true)).toBe(true);});
});
