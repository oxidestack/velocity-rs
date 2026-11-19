// Renders test cases with the real Apache Velocity 2.4.1 and velocity-tools 3.1, configured the way MockServer
// 8.0.0's VelocityTemplateEngine configures them, and prints what the Rust port must reproduce.
//
// Input (stdin): a JSON array of cases {"template": "...", "context": {...}}.
// Output (stdout): a JSON array of {"output": "..."} or {"error": "<exception class>: <message>"}, plus "tokens"
// (kind name, image, special-token images) for each case.
//
// Run: ./reference.sh < cases.json

import shaded_package.com.github.cliftonlabs.json_simple.JsonArray;
import shaded_package.com.github.cliftonlabs.json_simple.JsonObject;
import shaded_package.com.github.cliftonlabs.json_simple.Jsoner;
import org.apache.velocity.Template;
import org.apache.velocity.VelocityContext;
import org.apache.velocity.app.VelocityEngine;
import org.apache.velocity.context.InternalContextAdapter;
import org.apache.velocity.runtime.RuntimeConstants;
import org.apache.velocity.runtime.RuntimeInstance;
import org.apache.velocity.runtime.directive.Directive;
import org.apache.velocity.runtime.parser.StandardParserConstants;
import org.apache.velocity.runtime.parser.Token;
import org.apache.velocity.runtime.parser.node.Node;
import org.apache.velocity.runtime.parser.node.SimpleNode;
import org.apache.velocity.runtime.resource.loader.StringResourceLoader;
import org.apache.velocity.runtime.resource.util.StringResourceRepository;
import org.apache.velocity.tools.ToolManager;
import org.apache.velocity.tools.config.ToolConfiguration;
import org.apache.velocity.tools.config.ToolboxConfiguration;
import org.apache.velocity.tools.config.XmlFactoryConfiguration;

import java.io.InputStreamReader;
import java.io.StringReader;
import java.io.StringWriter;
import java.io.Writer;
import java.math.BigDecimal;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;

public class Reference {
    /** MockServer's `#ifnull($x) … #end`. */
    public static class Ifnull extends Directive {
        public String getName() { return "ifnull"; }
        public int getType() { return BLOCK; }
        public boolean render(InternalContextAdapter context, Writer writer, Node node) throws java.io.IOException {
            Object value = node.jjtGetChild(0).value(context);
            if (value == null) {
                node.jjtGetChild(1).render(context, writer);
            }
            return true;
        }
    }

    static final String REPOSITORY = "velocity-reference";

    static VelocityEngine engine() {
        Properties p = new Properties();
        p.put(RuntimeConstants.RUNTIME_LOG_REFERENCE_LOG_INVALID, "true");
        p.put(RuntimeConstants.RUNTIME_STRING_INTERNING, "true");
        p.put(RuntimeConstants.MAX_NUMBER_LOOPS, "-1");
        p.put(RuntimeConstants.CHECK_EMPTY_OBJECTS, "true");
        p.put(RuntimeConstants.PARSE_DIRECTIVE_MAXDEPTH, "10");
        p.put(RuntimeConstants.RUNTIME_REFERENCES_STRICT, "false");
        p.put("context.scope_control.template", "false");
        p.put("context.scope_control.evaluate", "false");
        p.put("context.scope_control.foreach", "true");
        p.put("context.scope_control.macro", "false");
        p.put("context.scope_control.define", "false");
        p.put("directive.set.null.allowed", "true");
        p.put(RuntimeConstants.INTERPOLATE_STRINGLITERALS, "true");
        p.put(RuntimeConstants.INPUT_ENCODING, "UTF-8");
        p.put(RuntimeConstants.SPACE_GOBBLING, "lines");
        p.put(RuntimeConstants.PARSER_HYPHEN_ALLOWED, "true");
        p.put(RuntimeConstants.CUSTOM_DIRECTIVES, Ifnull.class.getName());
        p.put(RuntimeConstants.RESOURCE_LOADERS, "string");
        p.put("resource.loader.string.class", StringResourceLoader.class.getName());
        p.put("resource.loader.string.repository.class", org.apache.velocity.runtime.resource.util.StringResourceRepositoryImpl.class.getName());
        p.put("resource.loader.string.repository.static", "true");
        p.put("resource.loader.string.repository.name", REPOSITORY);
        p.put("resource.loader.string.repository.encoding", "UTF-8");
        p.put("resource.loader.string.cache", "true");
        p.put("resource.loader.string.modification_check_interval", "0");
        p.put(RuntimeConstants.UBERSPECT_CLASSNAME, org.apache.velocity.util.introspection.SecureUberspector.class.getName());
        VelocityEngine engine = new VelocityEngine();
        engine.init(p);
        return engine;
    }

    static ToolManager tools(VelocityEngine engine) {
        ToolManager manager = new ToolManager();
        ToolboxConfiguration application = new ToolboxConfiguration();
        application.setScope("application");
        for (Class<?> c : new Class<?>[] {
            org.apache.velocity.tools.generic.CollectionTool.class, org.apache.velocity.tools.generic.ComparisonDateTool.class,
            org.apache.velocity.tools.generic.DisplayTool.class, org.apache.velocity.tools.generic.EscapeTool.class,
            org.apache.velocity.tools.generic.MathTool.class, org.apache.velocity.tools.generic.NumberTool.class}) {
            ToolConfiguration tool = new ToolConfiguration();
            tool.setClass(c.getName());
            application.addTool(tool);
        }
        ToolboxConfiguration request = new ToolboxConfiguration();
        request.setScope("request");
        for (Class<?> c : new Class<?>[] {org.apache.velocity.tools.generic.JsonTool.class, org.apache.velocity.tools.generic.XmlTool.class}) {
            ToolConfiguration tool = new ToolConfiguration();
            tool.setClass(c.getName());
            request.addTool(tool);
        }
        XmlFactoryConfiguration configuration = new XmlFactoryConfiguration();
        configuration.addToolbox(application);
        configuration.addToolbox(request);
        manager.configure(configuration);
        manager.setVelocityEngine(engine);
        return manager;
    }

    /** JSON into the Java values a MockServer template sees: String, Integer/Long/BigDecimal→Double, lists, maps. */
    static Object java(Object json) {
        if (json instanceof BigDecimal) {
            BigDecimal d = (BigDecimal) json;
            if (d.scale() <= 0) {
                try { return d.intValueExact(); } catch (ArithmeticException e) { return d.longValue(); }
            }
            return d.doubleValue();
        }
        if (json instanceof List) {
            List<Object> out = new ArrayList<>();
            for (Object o : (List<?>) json) out.add(java(o));
            return out;
        }
        if (json instanceof Map) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<?, ?> e : ((Map<?, ?>) json).entrySet()) out.put(String.valueOf(e.getKey()), java(e.getValue()));
            return out;
        }
        return json;
    }

    static JsonArray tokens(RuntimeInstance runtime, String template) {
        JsonArray out = new JsonArray();
        try {
            Template t = new Template();
            t.setName("tokens");
            SimpleNode root = runtime.parse(new StringReader(template), t);
            for (Token tok = root.getFirstToken(); tok != null; tok = tok.next) {
                JsonArray specials = new JsonArray();
                for (Token s = tok.specialToken; s != null; s = s.specialToken) specials.add(0, s.image);
                JsonArray entry = new JsonArray();
                entry.add(StandardParserConstants.tokenImage[tok.kind]);
                entry.add(tok.image);
                if (!specials.isEmpty()) entry.add(specials);
                out.add(entry);
                if (tok.kind == StandardParserConstants.EOF) break;
            }
        } catch (Throwable e) {
            out.add("ERROR " + e.getClass().getName() + ": " + e.getMessage());
        }
        return out;
    }

    public static void main(String[] args) throws Exception {
        JsonArray cases = (JsonArray) Jsoner.deserialize(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        VelocityEngine engine = engine();
        ToolManager tools = tools(engine);
        StringResourceRepository repository = StringResourceLoader.getRepository(REPOSITORY);
        RuntimeInstance tokenRuntime = new RuntimeInstance();
        Properties tp = new Properties();
        tp.put(RuntimeConstants.PARSER_HYPHEN_ALLOWED, "true");
        tp.put(RuntimeConstants.SPACE_GOBBLING, "lines");
        tokenRuntime.init(tp);
        JsonArray results = new JsonArray();
        for (Object c : cases) {
            JsonObject in = (JsonObject) c;
            String template = (String) in.get("template");
            JsonObject result = new JsonObject();
            try {
                Map<String, Object> vars = new LinkedHashMap<>();
                Object ctx = in.get("context");
                if (ctx instanceof Map) {
                    for (Map.Entry<?, ?> e : ((Map<?, ?>) ctx).entrySet()) vars.put(String.valueOf(e.getKey()), java(e.getValue()));
                }
                VelocityContext functionsLayer = new VelocityContext(vars, tools.createContext());
                VelocityContext context = new VelocityContext(functionsLayer);
                repository.putStringResource(template, template, "UTF-8");
                StringWriter writer = new StringWriter();
                engine.getTemplate(template, "UTF-8").merge(context, writer);
                result.put("output", writer.toString());
            } catch (Throwable e) {
                result.put("error", e.getClass().getName() + ": " + e.getMessage());
            }
            if (Boolean.TRUE.equals(in.get("tokens"))) {
                result.put("tokens", tokens(tokenRuntime, template));
            }
            results.add(result);
        }
        System.out.println(Jsoner.prettyPrint(results.toJson()));
    }
}
