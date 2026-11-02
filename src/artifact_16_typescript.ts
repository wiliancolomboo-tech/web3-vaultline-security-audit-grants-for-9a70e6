import * as crypto from "crypto";
import * as fs from "fs";

export class ArtifactHasher {
  public static hashFile(filePath: string): Buffer {
    if (!fs.existsSync(filePath)) {
      throw new Error(`Audit file not found at: ${filePath}`);
    }
    const fileBuffer = fs.readFileSync(filePath);
    return crypto.createHash("sha256").update(fileBuffer).digest();
  }

  public static hashBuffer(data: Buffer): Buffer {
    return crypto.createHash("sha256").update(data).digest();
  }

  public static parseUuidTo16Bytes(uuidStr: string): number[] {
    const clean = uuidStr.replace(/-/g, "");
    if (clean.length !== 32) {
      throw new Error("Invalid UUID length");
    }
    const bytes: number[] = [];
    for (let i = 0; i < 32; i += 2) {
      bytes.push(parseInt(clean.substr(i, 2), 16));
    }
    return bytes;
  }
}