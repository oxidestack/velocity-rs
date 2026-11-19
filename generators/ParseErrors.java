// Parses templates with the real Apache Velocity 2.4.1, configured the way MockServer 8.0.0 configures it, and prints
// what the Rust port must reproduce: the exception class and message of a template that does not parse.
//
// Input (stdin): one template per line, base64 of its UTF-8 bytes. Output (stdout): one line per template, base64 of
// "O" (rendered fine) or "E<exception class>: <message>". Every template gets a fresh engine, so macros one template
// defines are not known to the next (the oracle's own engine is shared, which couples recorded cases to their order).
//
// Run: generators/parse_errors.sh < templates.b64
//
// With the argument "template" each template is instead loaded through a string resource loader under its own text as
// the name (getTemplate, as MockServer's VelocityTemplateEngine does), which names the template in lexical errors.

import org.apache.velocity.VelocityContext;
import org.apache.velocity.app.VelocityEngine;
import org.apache.velocity.context.InternalContextAdapter;
import org.apache.velocity.runtime.RuntimeConstants;
import org.apache.velocity.runtime.directive.Directive;
import org.apache.velocity.runtime.parser.node.Node;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.io.StringWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.Properties;

public class ParseErrors {
    /** MockServer's `#ifnull($x) ... #end`. */
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

    static VelocityEngine engine(boolean template) {
        Properties p = new Properties();
        if (template) {
            p.put("resource.loaders", "string");
            p.put("resource.loader.string.class", org.apache.velocity.runtime.resource.loader.StringResourceLoader.class.getName());
        }
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
        p.put(RuntimeConstants.UBERSPECT_CLASSNAME, org.apache.velocity.util.introspection.SecureUberspector.class.getName());
        VelocityEngine engine = new VelocityEngine();
        engine.init(p);
        return engine;
    }

    public static void main(String[] args) throws Exception {
        boolean viaTemplate = args.length > 0 && args[0].equals("template");
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        PrintStream out = new PrintStream(System.out, false, StandardCharsets.UTF_8);
        String line;
        while ((line = in.readLine()) != null) {
            String template = new String(Base64.getDecoder().decode(line), StandardCharsets.UTF_8);
            String result;
            try {
                if (viaTemplate) {
                    VelocityEngine engine = engine(true);
                    org.apache.velocity.runtime.resource.loader.StringResourceLoader.getRepository().putStringResource(template, template, "UTF-8");
                    engine.getTemplate(template, "UTF-8").merge(new VelocityContext(), new StringWriter());
                } else {
                    engine(false).evaluate(new VelocityContext(), new StringWriter(), template, template);
                }
                result = "O";
            } catch (Throwable e) {
                result = "E" + e.getClass().getName() + ": " + e.getMessage();
            }
            out.println(Base64.getEncoder().encodeToString(result.getBytes(StandardCharsets.UTF_8)));
        }
        out.flush();
    }
}
