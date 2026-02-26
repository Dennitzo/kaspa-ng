export declare function checkStatus(): Promise<{ isAvailable: boolean }>;
export declare function hasData(_args: { domain: string; name: string }): Promise<boolean>;
export declare function getData(_args: { domain: string; name: string; reason?: string; cancelTitle?: string }): Promise<{ data: string } | null>;
export declare function setData(_args: { domain: string; name: string; data: string }): Promise<void>;
