const languages:Record<string,string>={ts:"typescript",tsx:"typescript",js:"javascript",jsx:"javascript",json:"json",css:"css",scss:"scss",html:"html",md:"markdown",rs:"rust",py:"python",gd:"gdscript",toml:"ini",yaml:"yaml",yml:"yaml",xml:"xml",sh:"shell"};
export function languageFor(path:string){const extension=path.split(".").pop()?.toLowerCase()??"";return languages[extension]??"plaintext";}
