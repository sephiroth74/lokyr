// The type checker of a plugin's sources, for Sleip: Luau's frontend of the
// sources in memory, each module by its path in the package, in strict
// mode, of the solver that luau-lsp takes by default, the old one, the
// plugin API's definitions loaded as the globals' types; `require` of a
// path from `./` or `../` within the package, of `.luau` or a folder's
// `init.luau`. What it finds, a problem a
// record, its module, line, column, kind and message, of fields of 0x1f and
// records of 0x1e, which Rust reads.

#include "Luau/BuiltinDefinitions.h"
#include "Luau/Config.h"
#include "Luau/ConfigResolver.h"
#include "Luau/Error.h"
#include "Luau/FileResolver.h"
#include "Luau/Frontend.h"

#include <cstdlib>
#include <cstring>
#include <string>
#include <unordered_map>
#include <vector>

namespace
{

// The sources, by their paths in the package.
struct Sources : Luau::FileResolver
{
    std::unordered_map<std::string, std::string> files;

    std::optional<Luau::SourceCode> readSource(const Luau::ModuleName& name) override
    {
        auto found = files.find(name);
        if (found == files.end())
            return std::nullopt;
        return Luau::SourceCode{found->second, Luau::SourceCode::Module};
    }

    std::optional<Luau::ModuleInfo> resolveModule(
        const Luau::ModuleInfo* context,
        Luau::AstExpr* node,
        const Luau::TypeCheckLimits& limits
    ) override
    {
        Luau::AstExprConstantString* expr = node->as<Luau::AstExprConstantString>();
        if (!expr || !context)
            return std::nullopt;
        std::string path{expr->value.data, expr->value.size};
        if (path.rfind("./", 0) != 0 && path.rfind("../", 0) != 0)
            return std::nullopt;
        // The requiring module's folder, of its path's parts.
        std::vector<std::string> parts;
        std::string from = context->name;
        size_t start = 0;
        for (size_t at = from.find('/'); at != std::string::npos; at = from.find('/', start))
        {
            parts.push_back(from.substr(start, at - start));
            start = at + 1;
        }
        start = 0;
        for (size_t at = path.find('/'); ; at = path.find('/', start))
        {
            std::string part = path.substr(start, at == std::string::npos ? std::string::npos : at - start);
            if (part == "..")
            {
                if (parts.empty())
                    return std::nullopt;
                parts.pop_back();
            }
            else if (!part.empty() && part != ".")
                parts.push_back(part);
            if (at == std::string::npos)
                break;
            start = at + 1;
        }
        std::string base;
        for (const std::string& part : parts)
            base += (base.empty() ? "" : "/") + part;
        for (const std::string& candidate : {base + ".luau", base + "/init.luau"})
            if (files.count(candidate))
                return Luau::ModuleInfo{candidate};
        return std::nullopt;
    }
};

// Every module in strict mode, as a plugin's sources are.
struct Strict : Luau::ConfigResolver
{
    Luau::Config config;

    Strict()
    {
        config.mode = Luau::Mode::Strict;
    }

    const Luau::Config& getConfig(const Luau::ModuleName& name, const Luau::TypeCheckLimits& limits) const override
    {
        return config;
    }
};

// A field of a record, of no separator in it.
std::string field(std::string text)
{
    for (char& c : text)
        if (c == '\x1e' || c == '\x1f')
            c = ' ';
    return text;
}

// A record of a problem.
void record(std::string& out, const std::string& module, const Luau::Location& location, const char* kind, const std::string& message)
{
    out += field(module) + '\x1f' + std::to_string(location.begin.line + 1) + '\x1f' + std::to_string(location.begin.column + 1) + '\x1f' +
           kind + '\x1f' + field(message) + '\x1e';
}

char* copy(const std::string& text)
{
    char* copied = static_cast<char*>(std::malloc(text.size() + 1));
    if (copied)
        std::memcpy(copied, text.c_str(), text.size() + 1);
    return copied;
}

} // namespace

// Checks the `count` modules of `names` and `texts` against the
// definitions `definitions`, of `definitions_size` bytes, without the
// `taken_count` globals of `taken`, which the host takes away: the records
// of the problems found, which `sleip_luau_free` frees, and their count in
// `problems`.
extern "C" char* sleip_luau_check(
    const char* const* names,
    const char* const* texts,
    size_t count,
    const char* definitions,
    size_t definitions_size,
    const char* const* taken,
    size_t taken_count,
    size_t* problems
)
{
    std::string out;
    size_t found = 0;
    try
    {
        Sources sources;
        std::vector<std::string> modules;
        for (size_t i = 0; i < count; ++i)
        {
            sources.files[names[i]] = texts[i];
            modules.push_back(names[i]);
        }
        Strict config;
        Luau::FrontendOptions options;
        options.runLintChecks = false;
        Luau::Frontend frontend(Luau::SolverMode::Old, &sources, &config, options);
        Luau::registerBuiltinGlobals(frontend, frontend.globals);
        // The globals that the host takes away, which a call of is an
        // unknown global's.
        for (size_t i = 0; i < taken_count; ++i)
        {
            Luau::AstName name = frontend.globals.globalNames.names->get(taken[i]);
            if (name.value)
                frontend.globals.globalScope->bindings.erase(name);
        }
        Luau::LoadDefinitionFileResult loaded = frontend.loadDefinitionFile(
            frontend.globals, frontend.globals.globalScope, std::string_view(definitions, definitions_size), "@lokyr", false, false
        );
        if (!loaded.success)
        {
            for (const Luau::ParseError& error : loaded.parseResult.errors)
            {
                record(out, "lokyr.d.luau", error.getLocation(), "SyntaxError", error.getMessage());
                ++found;
            }
            if (found == 0)
            {
                record(out, "lokyr.d.luau", Luau::Location(), "TypeError", "the plugin API's definitions do not type-check");
                ++found;
            }
        }
        Luau::freeze(frontend.globals.globalTypes);
        frontend.queueModuleCheck(modules);
        frontend.checkQueuedModules();
        for (const std::string& module : modules)
        {
            std::optional<Luau::CheckResult> result = frontend.getCheckResult(module, false);
            if (!result)
                continue;
            for (const Luau::TypeError& error : result->errors)
            {
                if (const Luau::SyntaxError* syntax = Luau::get_if<Luau::SyntaxError>(&error.data))
                    record(out, error.moduleName, error.location, "SyntaxError", syntax->message);
                else
                    record(out, error.moduleName, error.location, "TypeError", Luau::toString(error, Luau::TypeErrorToStringOptions{&sources}));
                ++found;
            }
        }
    }
    catch (const std::exception& error)
    {
        record(out, "", Luau::Location(), "InternalError", error.what());
        ++found;
    }
    *problems = found;
    return copy(out);
}

// Frees what `sleip_luau_check` returned.
extern "C" void sleip_luau_free(char* text)
{
    std::free(text);
}
