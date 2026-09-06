#define NOMINMAX
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdlib>
#include <iostream>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>
 #include <algorithm>
#include <cctype>
#include "miniz.h"

// 不使用 std::filesystem：MinGW 对含中文/空格路径的行为不稳定。
// 全部使用 Windows 宽字符 API。

// ============ UTF-8 <-> UTF-16 转换 ============

std::wstring utf8ToWstr(const std::string& s) {
    if (s.empty()) return L"";
    int wlen = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), NULL, 0);
    if (wlen <= 0) return L"";
    std::wstring wstr(wlen, 0);
    MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), &wstr[0], wlen);
    return wstr;
}

std::string wstrToUtf8(const std::wstring& ws) {
    if (ws.empty()) return "";
    int ulen = WideCharToMultiByte(CP_UTF8, 0, ws.c_str(), (int)ws.size(), NULL, 0, NULL, NULL);
    if (ulen <= 0) return "";
    std::string str(ulen, '\0');
    WideCharToMultiByte(CP_UTF8, 0, ws.c_str(), (int)ws.size(), &str[0], ulen, NULL, NULL);
    return str;
}

// ============ 路径/文件系统操作（宽字符） ============

bool pathExists(const std::wstring& wpath) {
    DWORD attr = GetFileAttributesW(wpath.c_str());
    return attr != INVALID_FILE_ATTRIBUTES && !(attr & FILE_ATTRIBUTE_DIRECTORY);
}

bool createParentDirs(const std::wstring& wpath) {
    size_t slash = wpath.find_last_of(L"\\/");
    if (slash == std::wstring::npos || slash == 0) return true;
    std::wstring parent = wpath.substr(0, slash);
    // 递归创建
    if (CreateDirectoryW(parent.c_str(), NULL)) return true;
    DWORD err = GetLastError();
    if (err == ERROR_PATH_NOT_FOUND) {
        createParentDirs(parent);
        return CreateDirectoryW(parent.c_str(), NULL) != 0 || GetLastError() == ERROR_ALREADY_EXISTS;
    }
    return err == ERROR_ALREADY_EXISTS;
}

bool deleteFileW(const std::wstring& wpath) {
    return DeleteFileW(wpath.c_str()) != 0;
}

std::string getExtension(const std::string& path) {
    size_t dot = path.find_last_of('.');
    size_t sep = path.find_last_of("\\/");
    if (dot == std::string::npos || (sep != std::string::npos && dot < sep)) return "";
    std::string ext = path.substr(dot);
    for (char& c : ext) c = (char)std::tolower((unsigned char)c);
    return ext;
}

// ============ 编码检测 ============
enum class Encoding { UTF8, UTF16LE, UTF16BE, GBK, ASCII };

Encoding detectEncoding(const std::vector<char>& data) {
    size_t n = data.size();
    if (n >= 3 && (unsigned char)data[0] == 0xEF &&
        (unsigned char)data[1] == 0xBB && (unsigned char)data[2] == 0xBF)
        return Encoding::UTF8;
    if (n >= 2 && (unsigned char)data[0] == 0xFF && (unsigned char)data[1] == 0xFE)
        return Encoding::UTF16LE;
    if (n >= 2 && (unsigned char)data[0] == 0xFE && (unsigned char)data[1] == 0xFF)
        return Encoding::UTF16BE;

    bool hasNonAscii = false;
    for (size_t i = 0; i < n && i < 8192; i++) {
        unsigned char c = (unsigned char)data[i];
        if (c >= 0x80) { hasNonAscii = true; break; }
    }
    if (!hasNonAscii) return Encoding::ASCII;

    for (size_t i = 0; i < n && i < 8192; i++) {
        unsigned char c = (unsigned char)data[i];
        if (c < 0x80) continue;
        int seqLen;
        if ((c & 0xE0) == 0xC0) seqLen = 2;
        else if ((c & 0xF0) == 0xE0) seqLen = 3;
        else if ((c & 0xF8) == 0xF0) seqLen = 4;
        else return Encoding::GBK;
        for (int j = 1; j < seqLen; j++) {
            if (i + j >= n || ((unsigned char)data[i + j] & 0xC0) != 0x80)
                return Encoding::GBK;
        }
        i += seqLen - 1;
    }
    return Encoding::UTF8;
}

// ============ 编码转 UTF-8 ============
std::string toUtf8(const std::vector<char>& data, Encoding enc, bool stripBom) {
    size_t bomSkip = 0;
    if (stripBom) {
        if (enc == Encoding::UTF8 && data.size() >= 3 &&
            (unsigned char)data[0] == 0xEF &&
            (unsigned char)data[1] == 0xBB &&
            (unsigned char)data[2] == 0xBF)
            bomSkip = 3;
        else if ((enc == Encoding::UTF16LE || enc == Encoding::UTF16BE) && data.size() >= 2 &&
                 (((unsigned char)data[0] == 0xFF && (unsigned char)data[1] == 0xFE) ||
                  ((unsigned char)data[0] == 0xFE && (unsigned char)data[1] == 0xFF)))
            bomSkip = 2;
    }

    if (enc == Encoding::ASCII || enc == Encoding::UTF8)
        return std::string(data.begin() + bomSkip, data.end());

    if (enc == Encoding::UTF16LE || enc == Encoding::UTF16BE) {
        std::wstring wstr;
        for (size_t i = bomSkip; i + 1 < data.size(); i += 2) {
            wchar_t ch = (enc == Encoding::UTF16LE)
                ? ((unsigned char)data[i] | ((unsigned char)data[i + 1] << 8))
                : (((unsigned char)data[i] << 8) | (unsigned char)data[i + 1]);
            if (ch != 0) wstr += ch;
        }
        if (wstr.empty()) return "";
        int ulen = WideCharToMultiByte(CP_UTF8, 0, wstr.c_str(), (int)wstr.size(), NULL, 0, NULL, NULL);
        if (ulen <= 0) return "";
        std::string result(ulen, '\0');
        WideCharToMultiByte(CP_UTF8, 0, wstr.c_str(), (int)wstr.size(), &result[0], ulen, NULL, NULL);
        return result;
    }

    if (enc == Encoding::GBK) {
        int wlen = MultiByteToWideChar(936, 0, data.data() + bomSkip, (int)(data.size() - bomSkip), NULL, 0);
        if (wlen <= 0) return std::string(data.begin() + bomSkip, data.end());
        std::wstring wstr(wlen, 0);
        MultiByteToWideChar(936, 0, data.data() + bomSkip, (int)(data.size() - bomSkip), &wstr[0], wlen);
        int ulen = WideCharToMultiByte(CP_UTF8, 0, wstr.c_str(), (int)wstr.size(), NULL, 0, NULL, NULL);
        if (ulen <= 0) return std::string(data.begin() + bomSkip, data.end());
        std::string result(ulen, '\0');
        WideCharToMultiByte(CP_UTF8, 0, wstr.c_str(), (int)wstr.size(), &result[0], ulen, NULL, NULL);
        return result;
    }
    return std::string(data.begin() + bomSkip, data.end());
}

// ============ 读文件到内存（宽字符） ============
bool readFileData(const std::wstring& wpath, std::vector<char>& data, Encoding& enc) {
    HANDLE hFile = CreateFileW(wpath.c_str(), GENERIC_READ, FILE_SHARE_READ, NULL,
                               OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (hFile == INVALID_HANDLE_VALUE) return false;

    LARGE_INTEGER size;
    if (!GetFileSizeEx(hFile, &size) || size.QuadPart > 50 * 1024 * 1024) {
        CloseHandle(hFile);
        std::cerr << "ERROR: file too large (>50MB)" << std::endl;
        return false;
    }
    size_t fsize = (size_t)size.QuadPart;
    data.resize(fsize);

    DWORD totalRead = 0;
    while (totalRead < fsize) {
        DWORD toRead = (DWORD)std::min<size_t>(fsize - totalRead, 0x7FFFFFFF);
        DWORD read = 0;
        if (!ReadFile(hFile, data.data() + totalRead, toRead, &read, NULL)) {
            CloseHandle(hFile);
            return false;
        }
        if (read == 0) break;
        totalRead += read;
    }
    CloseHandle(hFile);
    enc = detectEncoding(data);
    return true;
}

// ============ 写文件（宽字符） ============
bool writeFileData(const std::wstring& wpath, const char* buf, size_t len) {
    createParentDirs(wpath);
    HANDLE hFile = CreateFileW(wpath.c_str(), GENERIC_WRITE, 0, NULL,
                               CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (hFile == INVALID_HANDLE_VALUE) return false;

    size_t written = 0;
    while (written < len) {
        DWORD toWrite = (DWORD)std::min<size_t>(len - written, 0x7FFFFFFF);
        DWORD w = 0;
        if (!WriteFile(hFile, buf + written, toWrite, &w, NULL)) {
            CloseHandle(hFile);
            return false;
        }
        if (w == 0) break;
        written += w;
    }
    CloseHandle(hFile);
    return written == len;
}

// ============ read 命令 ============
int cmd_read(int argc, wchar_t* argvW[]) {
    if (argc < 3) {
        std::cerr << "Usage: file_ops read <path> [startLine] [endLine]" << std::endl;
        return 1;
    }
    std::string path = wstrToUtf8(argvW[2]);
    int startLine = (argc > 3) ? std::wcstol(argvW[3], NULL, 10) : 0;
    int endLine = (argc > 4) ? std::wcstol(argvW[4], NULL, 10) : 0;

    std::wstring wpath = utf8ToWstr(path);
    if (!pathExists(wpath)) {
        std::cerr << "ERROR: file not found: " << path << std::endl;
        return 1;
    }

    std::vector<char> data;
    Encoding enc;
    if (!readFileData(wpath, data, enc)) {
        std::cerr << "ERROR: cannot read file: " << path << std::endl;
        return 1;
    }
    if (data.empty()) { std::cout << "[empty file]" << std::endl; return 0; }

    std::string content = toUtf8(data, enc, true);

    // 分行
    std::vector<size_t> lineStarts;
    lineStarts.push_back(0);
    for (size_t i = 0; i < content.size(); i++) {
        if (content[i] == '\n') lineStarts.push_back(i + 1);
        else if (content[i] == '\r' && (i + 1 >= content.size() || content[i + 1] != '\n'))
            lineStarts.push_back(i + 1);
    }

    int total = (int)lineStarts.size();
    if (!content.empty() && content.back() == '\n' && !lineStarts.empty())
        total--;

    int s = (startLine > 0) ? startLine - 1 : 0;
    int e = (endLine > 0) ? std::min(endLine, total) : total;
    if (s >= total) {
        std::cerr << "ERROR: startLine " << startLine << " > total lines " << total << std::endl;
        return 1;
    }

    for (int i = s; i < e && i < total; i++) {
        size_t lineStart = lineStarts[i];
        size_t lineEnd = (i + 1 < (int)lineStarts.size()) ? lineStarts[i + 1] - 1 : content.size();
        if (lineEnd > lineStart && content[lineEnd - 1] == '\r') lineEnd--;
        std::cout << (i + 1) << ": " << content.substr(lineStart, lineEnd - lineStart) << "\n";
    }
    return 0;
}

// ============ write 命令 ============
int cmd_write(int argc, wchar_t* argvW[]) {
    if (argc < 3) {
        std::cerr << "Usage: file_ops write <path>" << std::endl;
        std::cerr << "  Content is read from stdin." << std::endl;
        return 1;
    }
    std::string path = wstrToUtf8(argvW[2]);

    // 从 stdin 读取全部内容
    std::vector<char> buffer;
    char chunk[65536];
    while (std::cin.read(chunk, sizeof(chunk)) || std::cin.gcount() > 0) {
        buffer.insert(buffer.end(), chunk, chunk + std::cin.gcount());
    }

    std::wstring wpath = utf8ToWstr(path);
    if (!writeFileData(wpath, buffer.data(), buffer.size())) {
        std::cerr << "ERROR: cannot write: " << path << std::endl;
        return 1;
    }

    // 计算行数
    int lineCount = buffer.empty() ? 0 : 1;
    for (char c : buffer) if (c == '\n') lineCount++;
    if (!buffer.empty() && buffer.back() == '\n') lineCount--;

    std::cout << "OK: " << path << " (" << lineCount << " lines, "
              << buffer.size() << " bytes)" << std::endl;
    return 0;
}

// ============ delete 命令 ============
int cmd_delete(int argc, wchar_t* argvW[]) {
    if (argc < 3) {
        std::cerr << "Usage: file_ops delete <path>" << std::endl;
        return 1;
    }
    std::string path = wstrToUtf8(argvW[2]);

    std::wstring wpath = utf8ToWstr(path);
    if (!pathExists(wpath)) {
        std::cerr << "ERROR: file not found: " << path << std::endl;
        return 1;
    }

    if (!deleteFileW(wpath)) {
        std::cerr << "ERROR: cannot delete: " << path << std::endl;
        return 1;
    }
    std::cout << "OK: deleted " << path << std::endl;
    return 0;
}

// ============ replace 命令 ============
// stdin 格式：search文本<<<REPLACE>>>replace文本
int cmd_replace(int argc, wchar_t* argvW[]) {
    if (argc < 3) {
        std::cerr << "Usage: file_ops replace <path>" << std::endl;
        std::cerr << "  Search/replace read from stdin, separated by <<<REPLACE>>>" << std::endl;
        return 1;
    }
    std::string path = wstrToUtf8(argvW[2]);

    std::wstring wpath = utf8ToWstr(path);
    if (!pathExists(wpath)) {
        std::cerr << "ERROR: file not found: " << path << std::endl;
        return 1;
    }

    // 从 stdin 读取 search 和 replace
    std::string stdinData;
    {
        std::ostringstream oss;
        char chunk[65536];
        while (std::cin.read(chunk, sizeof(chunk)) || std::cin.gcount() > 0)
            oss.write(chunk, std::cin.gcount());
        stdinData = oss.str();
    }

    const std::string SEP = "<<<REPLACE>>>";
    size_t sepPos = stdinData.find(SEP);
    if (sepPos == std::string::npos) {
        std::cerr << "ERROR: input must contain '" << SEP << "' separator" << std::endl;
        return 1;
    }
    std::string search = stdinData.substr(0, sepPos);
    std::string replace = stdinData.substr(sepPos + SEP.size());
    while (!replace.empty() && replace.back() == '\n') replace.pop_back();

    if (search.empty()) {
        std::cerr << "ERROR: search string cannot be empty" << std::endl;
        return 1;
    }

    std::vector<char> rawData;
    Encoding origEnc;
    if (!readFileData(wpath, rawData, origEnc)) {
        std::cerr << "ERROR: cannot read file: " << path << std::endl;
        return 1;
    }
    if (rawData.empty()) { std::cerr << "ERROR: file is empty" << std::endl; return 1; }

    std::string content = toUtf8(rawData, origEnc, true);

    // 统计替换次数
    size_t pos = 0;
    int count = 0;
    while ((pos = content.find(search, pos)) != std::string::npos) {
        count++;
        pos += search.length();
    }

    if (count == 0) {
        std::cerr << "ERROR: \"" << search << "\" not found in " << path << std::endl;
        return 1;
    }

    // 执行替换
    std::string newContent;
    pos = 0;
    size_t lastEnd = 0;
    while ((pos = content.find(search, lastEnd)) != std::string::npos) {
        newContent += content.substr(lastEnd, pos - lastEnd);
        newContent += replace;
        lastEnd = pos + search.length();
    }
    newContent += content.substr(lastEnd);

    if (!writeFileData(wpath, newContent.data(), newContent.size())) {
        std::cerr << "ERROR: cannot write: " << path << std::endl;
        return 1;
    }

    std::cout << "OK: " << path << " (" << count << " replacements)" << std::endl;
    return 0;
}

// ============ 二进制 Office 文件读取（C++ + ZIP/XML） ============

static bool readZipEntry(const std::vector<char>& zipData, const char* entryName, std::string& out) {
    mz_zip_archive zip;
    mz_zip_zero_struct(&zip);
    if (!mz_zip_reader_init_mem(&zip, zipData.data(), zipData.size(), 0)) return false;
    int idx = mz_zip_reader_locate_file(&zip, entryName, NULL, 0);
    if (idx < 0) { mz_zip_reader_end(&zip); return false; }
    mz_zip_archive_file_stat stat;
    if (!mz_zip_reader_file_stat(&zip, idx, &stat)) { mz_zip_reader_end(&zip); return false; }
    size_t size = (size_t)stat.m_uncomp_size;
    if (size == 0) { mz_zip_reader_end(&zip); out.clear(); return true; }
    std::vector<char> buf(size);
    if (!mz_zip_reader_extract_to_mem(&zip, idx, buf.data(), size, 0)) { mz_zip_reader_end(&zip); return false; }
    mz_zip_reader_end(&zip);
    out.assign(buf.data(), size);
    return true;
}

static std::vector<std::string> extractTagValues(const std::string& xml, const std::string& tag) {
    std::vector<std::string> results;
    std::string openStart = "<" + tag;
    std::string close = "</" + tag + ">";
    size_t pos = 0;
    while ((pos = xml.find(openStart, pos)) != std::string::npos) {
        size_t tagEnd = xml.find('>', pos);
        if (tagEnd == std::string::npos) break;
        if (xml[tagEnd - 1] == '/') { pos = tagEnd + 1; continue; }
        size_t closePos = xml.find(close, tagEnd + 1);
        if (closePos == std::string::npos) { pos = tagEnd + 1; continue; }
        std::string text = xml.substr(tagEnd + 1, closePos - tagEnd - 1);
        size_t nested = text.find(openStart);
        if (nested != std::string::npos) text = text.substr(0, nested);
        results.push_back(text);
        pos = closePos + close.size();
    }
    return results;
}

static std::string extractAttrValue(const std::string& xml, const std::string& attr) {
    std::string key = attr + "=\"";
    size_t pos = xml.find(key);
    if (pos == std::string::npos) return "";
    size_t start = pos + key.size();
    size_t end = xml.find('"', start);
    if (end == std::string::npos) return "";
    return xml.substr(start, end - start);
}

static std::string parseXlsx(const std::vector<char>& zipData) {
    std::string sharedStringsXml;
    readZipEntry(zipData, "xl/sharedStrings.xml", sharedStringsXml);
    std::vector<std::string> sharedStrings = extractTagValues(sharedStringsXml, "t");

    std::string workbookXml;
    readZipEntry(zipData, "xl/workbook.xml", workbookXml);
    std::vector<std::string> sheets;
    std::string sheetTag = "<sheet ";
    size_t pos = 0;
    while ((pos = workbookXml.find(sheetTag, pos)) != std::string::npos) {
        size_t tagEnd = workbookXml.find('>', pos);
        if (tagEnd == std::string::npos) break;
        std::string tag = workbookXml.substr(pos, tagEnd - pos + 1);
        sheets.push_back(extractAttrValue(tag, "name"));
        pos = tagEnd + 1;
    }

    std::ostringstream oss;
    int sheetNum = 1;
    while (true) {
        std::string entry = "xl/worksheets/sheet" + std::to_string(sheetNum) + ".xml";
        std::string sheetXml;
        if (!readZipEntry(zipData, entry.c_str(), sheetXml)) break;

        std::string sheetName = (sheetNum <= (int)sheets.size() && !sheets[sheetNum - 1].empty())
            ? sheets[sheetNum - 1] : ("Sheet" + std::to_string(sheetNum));
        oss << "=== " << sheetName << " ===\n";

        std::string rowOpen = "<row ";
        std::string rowClose = "</row>";
        size_t rpos = 0;
        int rowCount = 0;
        const int MAX_ROWS = 1000;
        while ((rpos = sheetXml.find(rowOpen, rpos)) != std::string::npos && rowCount < MAX_ROWS) {
            size_t rowEnd = sheetXml.find('>', rpos);
            if (rowEnd == std::string::npos) break;
            size_t rowClosePos = sheetXml.find(rowClose, rowEnd + 1);
            if (rowClosePos == std::string::npos) break;
            std::string rowXml = sheetXml.substr(rowEnd + 1, rowClosePos - rowEnd - 1);

            std::vector<std::string> cells;
            std::string cellOpen = "<c ";
            std::string cellClose = "</c>";
            size_t cpos = 0;
            while ((cpos = rowXml.find(cellOpen, cpos)) != std::string::npos) {
                size_t cellTagEnd = rowXml.find('>', cpos);
                if (cellTagEnd == std::string::npos) break;
                std::string cellTag = rowXml.substr(cpos, cellTagEnd - cpos + 1);
                size_t cellClosePos = rowXml.find(cellClose, cellTagEnd + 1);
                if (cellClosePos == std::string::npos) break;
                std::string cellXml = rowXml.substr(cellTagEnd + 1, cellClosePos - cellTagEnd - 1);

                std::string cellType = extractAttrValue(cellTag, "t");
                std::string val;
                if (cellType == "s") {
                    std::vector<std::string> vs = extractTagValues(cellXml, "v");
                    if (!vs.empty()) {
                        int idx = std::atoi(vs[0].c_str());
                        if (idx >= 0 && idx < (int)sharedStrings.size()) val = sharedStrings[idx];
                    }
                } else if (cellType == "inlineStr") {
                    std::vector<std::string> ts = extractTagValues(cellXml, "t");
                    if (!ts.empty()) val = ts[0];
                } else {
                    std::vector<std::string> vs = extractTagValues(cellXml, "v");
                    if (!vs.empty()) val = vs[0];
                }
                cells.push_back(val);
                cpos = cellClosePos + cellClose.size();
            }

            if (!cells.empty()) {
                for (size_t i = 0; i < cells.size(); i++) {
                    if (i > 0) oss << '\t';
                    oss << cells[i];
                }
                oss << '\n';
                rowCount++;
            }
            rpos = rowClosePos + rowClose.size();
        }
        sheetNum++;
    }
    return oss.str();
}

static std::string parseDocx(const std::vector<char>& zipData) {
    std::string docXml;
    if (!readZipEntry(zipData, "word/document.xml", docXml)) return "";

    std::ostringstream oss;
    std::string pOpen = "<w:p";
    std::string pClose = "</w:p>";
    size_t pos = 0;
    while ((pos = docXml.find(pOpen, pos)) != std::string::npos) {
        size_t tagEnd = docXml.find('>', pos);
        if (tagEnd == std::string::npos) break;
        size_t closePos = docXml.find(pClose, tagEnd + 1);
        if (closePos == std::string::npos) break;
        std::string pXml = docXml.substr(tagEnd + 1, closePos - tagEnd - 1);
        std::vector<std::string> ts = extractTagValues(pXml, "w:t");
        std::string para;
        for (const auto& t : ts) para += t;
        if (!para.empty()) oss << para << '\n';
        pos = closePos + pClose.size();
    }

    std::string tblOpen = "<w:tbl";
    std::string tblClose = "</w:tbl>";
    pos = 0;
    while ((pos = docXml.find(tblOpen, pos)) != std::string::npos) {
        size_t tagEnd = docXml.find('>', pos);
        if (tagEnd == std::string::npos) break;
        size_t closePos = docXml.find(tblClose, tagEnd + 1);
        if (closePos == std::string::npos) break;
        std::string tblXml = docXml.substr(tagEnd + 1, closePos - tagEnd - 1);
        std::string trOpen = "<w:tr";
        std::string trClose = "</w:tr>";
        size_t rpos = 0;
        while ((rpos = tblXml.find(trOpen, rpos)) != std::string::npos) {
            size_t trTagEnd = tblXml.find('>', rpos);
            if (trTagEnd == std::string::npos) break;
            size_t trClosePos = tblXml.find(trClose, trTagEnd + 1);
            if (trClosePos == std::string::npos) break;
            std::string trXml = tblXml.substr(trTagEnd + 1, trClosePos - trTagEnd - 1);
            std::vector<std::string> ts = extractTagValues(trXml, "w:t");
            for (size_t i = 0; i < ts.size(); i++) {
                if (i > 0) oss << '\t';
                oss << ts[i];
            }
            oss << '\n';
            rpos = trClosePos + trClose.size();
        }
        pos = closePos + tblClose.size();
    }
    return oss.str();
}

static std::string parsePptx(const std::vector<char>& zipData) {
    std::ostringstream oss;
    int slideNum = 1;
    while (true) {
        std::string entry = "ppt/slides/slide" + std::to_string(slideNum) + ".xml";
        std::string slideXml;
        if (!readZipEntry(zipData, entry.c_str(), slideXml)) break;
        oss << "--- Slide " << slideNum << " ---\n";
        std::vector<std::string> ts = extractTagValues(slideXml, "a:t");
        std::string text;
        for (const auto& t : ts) {
            if (!text.empty()) text += ' ';
            text += t;
        }
        if (!text.empty()) oss << text << '\n';
        slideNum++;
    }
    return oss.str();
}

// ============ binary 命令 ============
int cmd_binary(int argc, wchar_t* argvW[]) {
    if (argc < 3) {
        std::cerr << "Usage: file_ops binary <path>" << std::endl;
        return 1;
    }
    std::string path = wstrToUtf8(argvW[2]);
    std::wstring wpath = utf8ToWstr(path);
    if (!pathExists(wpath)) {
        std::cerr << "ERROR: file not found: " << path << std::endl;
        return 1;
    }

    std::vector<char> data;
    Encoding enc;
    if (!readFileData(wpath, data, enc)) {
        std::cerr << "ERROR: cannot read file: " << path << std::endl;
        return 1;
    }
    if (data.empty()) { std::cout << "[empty file]" << std::endl; return 0; }

    std::string ext = getExtension(path);
    std::string result;
    if (ext == ".xlsx") result = parseXlsx(data);
    else if (ext == ".docx") result = parseDocx(data);
    else if (ext == ".pptx") result = parsePptx(data);
    else if (ext == ".xls" || ext == ".doc" || ext == ".ppt") {
        std::cerr << "ERROR: legacy " << ext << " format is not supported, please convert to " << ext << "x" << std::endl;
        return 1;
    } else {
        std::cerr << "ERROR: unsupported binary type: " << ext << std::endl;
        return 1;
    }

    if (result.empty()) {
        std::cerr << "ERROR: no extractable content in " << path << std::endl;
        return 1;
    }

    std::istringstream iss(result);
    std::string line;
    int lineNo = 1;
    while (std::getline(iss, line)) {
        std::cout << lineNo << ": " << line << "\n";
        lineNo++;
    }
    return 0;
}

// ============ isbin 命令 ============
int cmd_isbin(int argc, wchar_t* argvW[]) {
    if (argc < 3) {
        std::cerr << "Usage: file_ops isbin <path>" << std::endl;
        return 1;
    }
    std::string ext = getExtension(wstrToUtf8(argvW[2]));

    static const std::vector<std::string> binExts = {
        ".pdf", ".xlsx", ".xls", ".docx", ".doc", ".pptx", ".ppt",
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".mp3", ".mp4"
    };

    if (std::find(binExts.begin(), binExts.end(), ext) != binExts.end()) {
        std::cout << ext << std::endl;
        return 0;
    }
    return 1;
}

// ============ main ============
int wmain(int argc, wchar_t* argvW[]) {
    // 设置控制台输入输出为 UTF-8
    SetConsoleOutputCP(CP_UTF8);
    SetConsoleCP(CP_UTF8);

    if (argc < 2) {
        std::cerr << "E-IDE File Operations Tool v3\n"
                  << "  read    <path> [startLine] [endLine]  读取文本文件\n"
                  << "  write   <path>                        写入文件(内容从stdin)\n"
                  << "  delete  <path>                        删除文件\n"
                  << "  replace <path>                        搜索替换(模式从stdin)\n"
                  << "  isbin   <path>                        检测二进制格式\n"
                  << "  binary  <path>                        读取xlsx/docx/pptx文本\n";
        return 1;
    }

    std::string cmd = wstrToUtf8(argvW[1]);
    if (cmd == "read")    return cmd_read(argc, argvW);
    if (cmd == "write")   return cmd_write(argc, argvW);
    if (cmd == "delete")  return cmd_delete(argc, argvW);
    if (cmd == "replace") return cmd_replace(argc, argvW);
    if (cmd == "isbin")   return cmd_isbin(argc, argvW);
    if (cmd == "binary")  return cmd_binary(argc, argvW);

    std::cerr << "ERROR: unknown command: " << cmd << std::endl;
    return 1;
}
